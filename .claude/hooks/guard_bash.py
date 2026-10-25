#!/usr/bin/env python3
"""Fail-closed guard for Claude Code Bash calls (PreToolUse hook).

Defense in depth, never a sandbox. The hook never approves anything: exit 0 only
means "no objection", and the normal permission system still decides. Exit 2
blocks the call. Every unexpected condition (bad payload, parser error, an
internal exception) also exits 2, because any other non-zero exit code would let
the command run.

The command is tokenized with a small shell lexer instead of regex-matching the
raw text, so quoted strings, heredoc bodies and documentation text never trigger
a block. Ambiguous constructs in security-relevant positions (variables or
command substitutions in a `git push` argument, an unresolved command name,
unbalanced quotes, ...) are blocked instead of guessed.

Profiles:
  project   deny-list for the main session (default)
  readonly  allow-list of read-only inspection commands (reviewer, architect)
  verify    readonly plus format/lint/typecheck/test/build commands (verifier)

Known limits: scripts and interpreters (`python3 -c`, `node -e`, `bash file.sh`)
run code this hook cannot see, and Edit/Write can create such scripts.
"""
import json
import os
import re
import subprocess
import sys

PROTECTED_BRANCHES = {"main", "master"}
SHELLS = {"sh", "bash", "zsh", "dash", "ksh", "fish"}
LOOP_KEYWORDS = {"for", "select", "case", "function", "coproc"}
LEADING_KEYWORDS = {"if", "then", "else", "elif", "fi", "while", "until", "do", "done", "!", "{", "}", "time"}
SECRET_READERS = {
    "cat", "head", "tail", "less", "more", "nl", "od", "xxd", "strings", "base64", "bat",
    "grep", "egrep", "rg", "sed", "awk", "cp", "scp", "tee",
}
SECRET_PATH = re.compile(r"(^|/)(\.env(\..*)?|[^/]*\.(p12|pfx|key|pem)|id_(rsa|ed25519|ecdsa))$")


class Block(Exception):
    """Raised to refuse a command; the message is shown to Claude."""


# --------------------------------------------------------------------------
# Lexer
# --------------------------------------------------------------------------
class Word:
    __slots__ = ("text", "var", "subst", "ansi", "glob", "quoted")

    def __init__(self):
        self.text = ""
        self.var = False    # unresolved $var / ${...}
        self.subst = False  # contains $(...) or backticks (analyzed separately)
        self.ansi = False   # $'...' quoting
        self.glob = False   # unquoted glob or brace-expansion metacharacter
        self.quoted = False # any part was quoted

    def dynamic(self):
        return self.var or self.subst or self.ansi

    def __repr__(self):
        return self.text


class Command:
    def __init__(self):
        self.words = []
        self.redirects = []        # (operator, target text)
        self.heredocs = []         # [delimiter, quoted, body]
        self.pipe_in = None        # previous Command when fed by a pipe
        self.herestring = None     # Word

    def argv(self):
        return [w.text for w in self.words]


def _matching_paren(s, i):
    """Return index of the ')' matching the '(' at s[i-1], respecting quotes."""
    depth, n = 1, len(s)
    while i < n:
        c = s[i]
        if c == "\\":
            i += 2
            continue
        if c == "'":
            j = s.find("'", i + 1)
            if j < 0:
                raise Block("unbalanced single quote inside command substitution")
            i = j + 1
            continue
        if c == '"':
            i += 1
            while i < n and s[i] != '"':
                i += 2 if s[i] == "\\" else 1
            i += 1
            continue
        if c == "<" and s.startswith("<<", i) and not s.startswith("<<<", i):
            m = re.match(r"<<-?[ \t]*(?:'([^']*)'|\"([^\"]*)\"|([A-Za-z0-9_]+))", s[i:])
            if m:
                delim = next(g for g in m.groups() if g is not None)
                nl = s.find("\n", i)
                if nl < 0:
                    raise Block("unterminated heredoc inside command substitution")
                pos = nl + 1
                while pos <= n:
                    k = s.find("\n", pos)
                    line = s[pos:] if k < 0 else s[pos:k]
                    pos = n + 1 if k < 0 else k + 1
                    if line.strip("\t") == delim:
                        break
                else:
                    raise Block("unterminated heredoc inside command substitution")
                i = pos
                continue
        if c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return i
        i += 1
    raise Block("unbalanced parenthesis in command substitution")


def _matching_backtick(s, i):
    n = len(s)
    while i < n:
        if s[i] == "\\":
            i += 2
            continue
        if s[i] == "`":
            return i
        i += 1
    raise Block("unbalanced backtick")


def parse(src, depth=0):
    """Parse shell text into Command objects; nested substitutions are returned too."""
    if depth > 8:
        raise Block("command nesting too deep to analyze")
    commands, nested = [], []
    cur = Command()
    word = None
    pending_redirect = None
    pending_heredocs = []
    i, n = 0, len(src)

    def end_word():
        nonlocal word, pending_redirect
        if word is None:
            return
        if pending_redirect is not None:
            cur.redirects.append((pending_redirect, word.text))
            if pending_redirect == "<<<":
                cur.herestring = word
            elif pending_redirect in ("<<", "<<-"):
                cur.heredocs.append([word.text, word.quoted, None])
                pending_heredocs.append((cur, cur.heredocs[-1], pending_redirect == "<<-"))
            pending_redirect = None
        else:
            cur.words.append(word)
        word = None

    def end_command(pipe=False):
        nonlocal cur
        end_word()
        if pending_redirect is not None:
            raise Block("redirection without a target")
        if cur.words or cur.redirects or cur.heredocs:
            commands.append(cur)
        nxt = Command()
        if pipe and commands:
            nxt.pipe_in = commands[-1]
        cur = nxt

    def ensure():
        nonlocal word
        if word is None:
            word = Word()
        return word

    while i < n:
        c = src[i]
        if c == "\\":
            if i + 1 >= n:
                raise Block("trailing backslash")
            if src[i + 1] == "\n":
                i += 2
                continue
            ensure().text += src[i + 1]
            i += 2
        elif c == "'":
            j = src.find("'", i + 1)
            if j < 0:
                raise Block("unbalanced single quote")
            w = ensure()
            w.quoted = True
            w.text += src[i + 1:j]
            i = j + 1
        elif c == '"':
            w = ensure()
            w.quoted = True
            i += 1
            while True:
                if i >= n:
                    raise Block("unbalanced double quote")
                d = src[i]
                if d == '"':
                    i += 1
                    break
                if d == "\\" and i + 1 < n:
                    if src[i + 1] in '"\\$`\n':
                        if src[i + 1] != "\n":
                            w.text += src[i + 1]
                    else:
                        w.text += d + src[i + 1]
                    i += 2
                elif d == "$" and src.startswith("$(", i):
                    j = _matching_paren(src, i + 2)
                    nested.append(src[i + 2:j])
                    w.subst = True
                    w.text += "$(...)"
                    i = j + 1
                elif d == "`":
                    j = _matching_backtick(src, i + 1)
                    nested.append(src[i + 1:j])
                    w.subst = True
                    w.text += "`...`"
                    i = j + 1
                elif d == "$" and i + 1 < n and (src[i + 1].isalpha() or src[i + 1] in "_{@*#?!$-0123456789"):
                    w.var = True
                    w.text += d
                    i += 1
                else:
                    w.text += d
                    i += 1
        elif c == "$" and src.startswith("$'", i):
            w = ensure()
            w.ansi = True
            w.quoted = True
            j = i + 2
            while j < n and src[j] != "'":
                j += 2 if src[j] == "\\" else 1
            if j >= n:
                raise Block("unbalanced $'...' quote")
            w.text += src[i + 2:j]
            i = j + 1
        elif c == "$" and src.startswith('$"', i):
            ensure()
            i += 1
        elif c == "$" and src.startswith("$(", i):
            j = _matching_paren(src, i + 2)
            nested.append(src[i + 2:j])
            w = ensure()
            w.subst = True
            w.text += "$(...)"
            i = j + 1
        elif c == "$" and i + 1 < n and (src[i + 1].isalpha() or src[i + 1] in "_{@*#?!$-0123456789"):
            w = ensure()
            w.var = True
            if src[i + 1] == "{":
                j = src.find("}", i)
                if j < 0:
                    raise Block("unbalanced ${...}")
                w.text += src[i:j + 1]
                i = j + 1
            else:
                w.text += c
                i += 1
        elif c == "`":
            j = _matching_backtick(src, i + 1)
            nested.append(src[i + 1:j])
            w = ensure()
            w.subst = True
            w.text += "`...`"
            i = j + 1
        elif c in "<>" and src.startswith("(", i + 1):
            j = _matching_paren(src, i + 2)
            nested.append(src[i + 2:j])
            w = ensure()
            w.subst = True
            w.text += c + "(...)"
            i = j + 1
        elif c in "<>":
            # Drop a purely numeric fd prefix such as the 2 in 2>&1.
            if word is not None and word.text.isdigit() and not (word.var or word.subst):
                word = None
            else:
                end_word()
            op = c
            if src.startswith("<<<", i):
                op = "<<<"
            elif src.startswith("<<", i):
                op = "<<-" if src.startswith("<<-", i) else "<<"
            elif src.startswith(">>", i):
                op = ">>"
            elif src.startswith("&>>", i):
                op = ">>"
            i += len(op)
            if op in (">", ">>", "<") and i < n and src[i] in "&|":
                if src[i] == "&":
                    op = op + "&"
                i += 1
                if src.startswith("-", i):
                    i += 1
                    continue
            if pending_redirect is not None:
                raise Block("consecutive redirections")
            pending_redirect = op
        elif c in " \t":
            end_word()
            i += 1
        elif c == "\n":
            end_command()
            i += 1
            for (_owner, record, strip_tabs) in pending_heredocs:
                delim_text = record[0]
                body_lines, found = [], False
                while i <= n:
                    k = src.find("\n", i)
                    line = src[i:] if k < 0 else src[i:k]
                    i = n + 1 if k < 0 else k + 1
                    if (line.lstrip("\t") if strip_tabs else line) == delim_text:
                        found = True
                        break
                    body_lines.append(line)
                if not found:
                    raise Block("unterminated heredoc")
                record[2] = "\n".join(body_lines)
                if not record[1]:
                    # An unquoted heredoc body is expanded by the shell: analyze its substitutions.
                    escaped = record[2].replace("\\", "\\\\").replace('"', '\\"')
                    nested.append("echo \"" + escaped + "\"")
            pending_heredocs = []
        elif c == "#" and word is None:
            while i < n and src[i] != "\n":
                i += 1
        elif c == ";" or c == "&" or c == "|":
            end_command(pipe=(c == "|" and not src.startswith("||", i)))
            if src.startswith("&&", i) or src.startswith("||", i) or src.startswith(";;", i) or src.startswith("|&", i):
                i += 2
            else:
                i += 1
        elif c in "(){}":
            nxt = src[i + 1] if i + 1 < n else " "
            if c in "{}":
                grouping = (c == "{" and word is None and nxt in " \t\n") or \
                           (c == "}" and word is None and nxt in " \t\n;&|)")
                if not grouping:
                    w = ensure()
                    w.glob = True  # brace expansion cannot be resolved statically
                    w.text += c
                    i += 1
                    continue
            end_command()
            i += 1
        else:
            w = ensure()
            if c in "*?[":
                w.glob = True
            w.text += c
            i += 1
    end_command()
    if pending_heredocs:
        raise Block("heredoc without body")
    return commands, nested


def analyze_all(src, depth=0):
    """Yield every simple command in src, including those in substitutions and heredoc bodies fed to shells."""
    commands, nested = parse(src, depth)
    for cmd in commands:
        yield cmd
    for inner in nested:
        yield from analyze_all(inner, depth + 1)


# --------------------------------------------------------------------------
# Command normalisation
# --------------------------------------------------------------------------
def strip_wrappers(words):
    """Return (words, inner) after removing env/command/nohup/... prefixes."""
    words = list(words)
    while words and words[0].text in LEADING_KEYWORDS:
        words.pop(0)
    while True:
        if not words:
            return words
        name = os.path.basename(words[0].text)
        if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*=.*", words[0].text):
            words.pop(0)
            continue
        if name in ("sudo", "doas", "su", "pkexec"):
            raise Block(f"privilege escalation via {name} is not allowed")
        if name in ("command", "builtin", "exec", "nohup", "stdbuf", "noglob", "setsid", "ionice", "caffeinate"):
            words.pop(0)
            while words and words[0].text.startswith("-") and len(words[0].text) > 1:
                words.pop(0)
            continue
        if name == "env":
            words.pop(0)
            while words and (words[0].text.startswith("-") or re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*=.*", words[0].text)):
                opt = words.pop(0).text
                if opt in ("-u", "-C", "-S") and words:
                    words.pop(0)
            continue
        if name in ("nice", "timeout"):
            words.pop(0)
            while words and words[0].text.startswith("-"):
                opt = words.pop(0).text
                if name == "nice" and opt == "-n" and words:
                    words.pop(0)
            if name == "timeout" and words:
                words.pop(0)  # duration
            continue
        return words


def git_subcommand(args):
    """Return (subcommand_word, rest_words) skipping git's global options."""
    i = 0
    while i < len(args):
        t = args[i].text
        if t in ("-C", "-c", "--git-dir", "--work-tree", "--namespace", "--exec-path", "--super-prefix", "--config-env"):
            if t == "-c" and i + 1 < len(args) and args[i + 1].text.lower().startswith("alias."):
                raise Block("git -c alias.* is not allowed")
            if t == "--exec-path":
                i += 1
                continue
            i += 2
            continue
        if t.startswith("-"):
            i += 1
            continue
        return args[i], args[i + 1:]
    return None, []


def short_flags(args):
    """Collect single-letter flags from clusters like -fd (ignores -- long options)."""
    out = set()
    for a in args:
        if a.startswith("--") or not a.startswith("-") or a == "-":
            continue
        out.update(a[1:])
    return out


def current_branch(cwd):
    try:
        r = subprocess.run(
            ["git", "symbolic-ref", "--short", "-q", "HEAD"],
            cwd=cwd or None, capture_output=True, text=True, timeout=5, check=False,
        )
        return r.stdout.strip() if r.returncode == 0 else None
    except (OSError, subprocess.SubprocessError):
        return None


def git_alias(cwd, name):
    try:
        r = subprocess.run(
            ["git", "config", "--get", f"alias.{name}"],
            cwd=cwd or None, capture_output=True, text=True, timeout=5, check=False,
        )
        return r.stdout.strip() if r.returncode == 0 else ""
    except (OSError, subprocess.SubprocessError):
        raise Block("could not resolve git aliases")


def check_git_push(rest, cwd):
    if any(w.dynamic() or w.glob for w in rest):
        raise Block("git push with variables, substitutions or globs cannot be verified; use literal arguments")
    args = [w.text for w in rest]
    positional, flags_done = [], False
    for a in args:
        if a == "--" and not flags_done:
            flags_done = True
            continue
        if not flags_done and a.startswith("--"):
            name = a.split("=", 1)[0]
            if name in ("--force", "--force-with-lease", "--force-if-includes", "--delete", "--mirror", "--prune"):
                raise Block(f"git push {name} is not allowed")
            if name == "--all":
                raise Block("git push --all would include protected branches")
            continue
        if not flags_done and a.startswith("-") and len(a) > 1:
            letters = a[1:]
            if "f" in letters or "d" in letters:
                raise Block("git push with force/delete flags is not allowed")
            continue
        positional.append(a)
    # positional[0] is the remote, the rest are refspecs.
    refspecs = positional[1:]
    if not refspecs:
        branch = current_branch(cwd)
        if branch is None or branch in PROTECTED_BRANCHES:
            raise Block("git push without an explicit refspec would target the current branch; "
                        "it is protected or could not be determined")
        return
    for spec in refspecs:
        if spec.startswith("+"):
            raise Block("forced refspec (+) is not allowed")
        src, sep, dst = spec.partition(":")
        if sep and src == "":
            raise Block("deleting a remote ref is not allowed")
        target = dst if sep else src
        if target == "HEAD":
            target = current_branch(cwd)
            if target is None:
                raise Block("cannot resolve HEAD for git push")
        target = re.sub(r"^refs/heads/", "", target)
        if target in PROTECTED_BRANCHES:
            raise Block(f"pushing to protected branch {target!r} is not allowed")
        if sep and re.sub(r"^refs/heads/", "", dst) in PROTECTED_BRANCHES:
            raise Block("pushing to a protected branch is not allowed")


def check_git(args_words, cwd):
    sub, rest = git_subcommand(args_words)
    if sub is None:
        return
    if sub.dynamic() or sub.glob:
        raise Block("git subcommand cannot be resolved statically")
    name = sub.text
    alias = git_alias(cwd, name)
    if alias:
        raise Block(f"git alias {name!r} expands to {alias!r}; run the real command instead")
    rest_text = [w.text for w in rest]
    flags = short_flags(rest_text)
    if name == "push":
        check_git_push(rest, cwd)
    elif name == "reset" and "--hard" in rest_text:
        raise Block("git reset --hard is not allowed")
    elif name == "clean" and ("f" in flags or any(a == "--force" for a in rest_text)):
        raise Block("git clean with force is not allowed")
    elif name in ("filter-branch", "filter-repo"):
        raise Block(f"git {name} rewrites history and is not allowed")
    elif name == "update-ref" and "-d" in rest_text:
        raise Block("git update-ref -d is not allowed")
    elif name == "branch" and ("D" in flags or "--delete" in rest_text and "--force" in rest_text):
        raise Block("force-deleting branches is not allowed")


GH_READ_ONLY_RELEASE = {"list", "view", "download"}


def check_gh(args_words):
    args = [w for w in args_words if not w.text.startswith("-")]
    if not args:
        return
    if any(w.dynamic() for w in args_words):
        raise Block("gh with variables or substitutions cannot be verified; use literal arguments")
    group = args[0].text
    action = args[1].text if len(args) > 1 else ""
    texts = [w.text for w in args_words]
    if group == "pr" and action == "merge":
        raise Block("gh pr merge is not allowed; the owner merges")
    if group == "release" and action not in GH_READ_ONLY_RELEASE:
        raise Block("gh release changes are not allowed")
    if group == "repo" and action in ("delete", "edit", "archive", "rename", "deploy-key"):
        raise Block(f"gh repo {action} is not allowed")
    if group in ("secret", "variable") and action in ("set", "delete", "remove"):
        raise Block(f"gh {group} {action} is not allowed")
    if group == "api":
        method = None
        for k, t in enumerate(texts):
            if t in ("-X", "--method") and k + 1 < len(texts):
                method = texts[k + 1].upper()
            elif t.startswith("--method="):
                method = t.split("=", 1)[1].upper()
        writes = any(t in ("-f", "-F", "--field", "--raw-field", "--input") or t.startswith(("--field=", "--raw-field=", "--input="))
                     for t in texts)
        if (method and method != "GET") or (writes and method is None):
            raise Block("mutating gh api calls (including implicit POST) are not allowed")


def check_rm(args_words):
    args = [w.text for w in args_words]
    if any(w.dynamic() for w in args_words):
        raise Block("rm with variables or substitutions cannot be verified; use literal paths")
    flags = short_flags(args)
    recursive = bool(flags & {"r", "R"}) or "--recursive" in args
    force = "f" in flags or "--force" in args
    targets = [a for a in args if not a.startswith("-")]
    dangerous = {"/", "~", ".", "..", "*", "./*", "/*", "~/", ".."}
    if recursive and (force or any(t in dangerous or t.endswith("/*") for t in targets)):
        raise Block("recursive rm with force or a broad target is not allowed")


def check_secret_read(name, args_words):
    if name not in SECRET_READERS:
        return
    for w in args_words:
        if w.text.startswith("-"):
            continue
        base = w.text
        if SECRET_PATH.search(base):
            raise Block(f"reading {base!r} through Bash is not allowed (see Read deny rules)")


def check_pipe_to_shell(cmd, name):
    if name in SHELLS and cmd.pipe_in is not None:
        src_words = strip_wrappers(cmd.pipe_in.words)
        src = os.path.basename(src_words[0].text) if src_words else ""
        if src in ("curl", "wget", "fetch", "nc", "ncat"):
            raise Block("piping downloaded content into a shell is not allowed")


def shell_payload(name, words):
    """For `sh -c STR` return STR's Word, else None. Raises when ambiguous."""
    flags_with_c = False
    idx = 1
    while idx < len(words):
        t = words[idx].text
        if t == "-c" or (t.startswith("-") and not t.startswith("--") and "c" in t[1:]):
            flags_with_c = True
            idx += 1
            break
        if t.startswith("-"):
            idx += 1
            continue
        break
    if not flags_with_c:
        return None
    if idx >= len(words):
        raise Block(f"{name} -c without a command string")
    payload = words[idx]
    if payload.dynamic():
        raise Block(f"{name} -c with a dynamic command string cannot be verified")
    return payload


def check_redirects(cmd, profile):
    if profile == "project":
        return
    for op, target in cmd.redirects:
        if op in (">", ">>") and target != "/dev/null":
            raise Block(f"redirecting output to {target!r} is not allowed in the {profile} profile")


def check_simple(cmd, profile, cwd):
    check_redirects(cmd, profile)
    words = strip_wrappers(cmd.words)
    if not words:
        return
    head = words[0]
    if head.var or head.ansi or head.subst or head.glob:
        raise Block("command name is dynamic and cannot be verified")
    name = os.path.basename(head.text)
    if name in LOOP_KEYWORDS:
        raise Block(f"shell construct {name!r} is not supported by the guard; run commands separately")
    args = words[1:]
    check_secret_read(name, args)
    check_pipe_to_shell(cmd, name)
    if name in SHELLS or name in ("source", "."):
        if any(w.text.startswith(("<(", ">(")) for w in args):
            raise Block(f"{name} reading a script from process substitution cannot be verified")
    if name in SHELLS:
        payload = shell_payload(name, words)
        if payload is not None:
            for inner in analyze_all(payload.text):
                check_simple(inner, profile, cwd)
        for _delim, _q, body in cmd.heredocs:
            for inner in analyze_all(body):
                check_simple(inner, profile, cwd)
        if cmd.herestring is not None:
            for inner in analyze_all(cmd.herestring.text):
                check_simple(inner, profile, cwd)
        if cmd.pipe_in is not None and not cmd.heredocs and cmd.herestring is None and payload is None:
            # A script arriving on stdin through a pipe cannot be inspected.
            raise Block("a shell reading its script from a pipe cannot be verified")
    elif name == "eval":
        text = " ".join(w.text for w in args)
        if any(w.dynamic() for w in args):
            raise Block("eval with a dynamic string cannot be verified")
        for inner in analyze_all(text):
            check_simple(inner, profile, cwd)
    elif name == "xargs":
        inner_words = list(args)
        while inner_words and inner_words[0].text.startswith("-"):
            opt = inner_words.pop(0).text
            if opt in ("-I", "-n", "-P", "-L", "-d", "-E", "-s") and inner_words:
                inner_words.pop(0)
        probe = Command()
        probe.words = inner_words
        # Extra stdin-supplied arguments are unknown: treat any dynamic sensitive tool as blocked.
        if inner_words:
            inner_name = os.path.basename(strip_wrappers(inner_words)[0].text) if strip_wrappers(inner_words) else ""
            if inner_name in ({"rm", "git", "gh"} | SHELLS):
                raise Block(f"xargs {inner_name} receives unverifiable arguments")
            check_simple(probe, profile, cwd)
    elif name == "find":
        texts = [w.text for w in args]
        if any(t in ("-delete", "-exec", "-execdir", "-ok", "-okdir", "-fprint", "-fprint0", "-fprintf", "-fls") for t in texts):
            if profile != "project":
                raise Block("find actions are not allowed in this profile")
            if "-delete" in texts:
                raise Block("find -delete is not allowed")
            for k, t in enumerate(texts):
                if t in ("-exec", "-execdir", "-ok", "-okdir"):
                    end = next((m for m in range(k + 1, len(texts)) if texts[m] in (";", "+", "\\;")), len(texts))
                    inner = Command()
                    inner.words = args[k + 1:end]
                    if inner.words:
                        check_simple(inner, profile, cwd)
    elif name == "git":
        check_git(args, cwd)
    elif name == "gh":
        check_gh(args)
    elif name == "rm":
        check_rm(args)
    if profile != "project":
        check_allowlist(name, cmd, words, profile)


# --------------------------------------------------------------------------
# Allow-list profiles (readonly / verify)
# --------------------------------------------------------------------------
READONLY_TOOLS = {"ls", "cat", "head", "tail", "wc", "grep", "egrep", "fgrep", "rg", "sort", "uniq", "file", "stat",
                  "pwd", "echo", "printf", "true", "false", "test", "basename", "dirname", "realpath", "tr", "cut",
                  "diff", "which", "type", "find"}
GIT_READONLY = {"status", "diff", "log", "show", "blame", "ls-files", "rev-parse", "rev-list", "merge-base", "describe",
                "shortlog", "ls-tree", "diff-tree", "cat-file", "grep", "show-ref", "name-rev", "cherry", "whatchanged"}
GIT_BRANCH_READ = {"-a", "-r", "-v", "-vv", "--list", "--show-current", "--all", "--remotes", "--verbose"}
PNPM_VERIFY = {"lint", "typecheck", "test", "build", "format:check"}
CARGO_VERIFY = {"clippy", "test", "check", "build"}


def check_allowlist(name, cmd, words, profile):
    args = [w.text for w in words[1:]]
    if name in READONLY_TOOLS:
        if name == "sort" and any(a == "-o" or a.startswith("--output") for a in args):
            raise Block("sort -o writes files")
        if name in ("rg", "grep", "egrep", "fgrep") and any(a.startswith("--pre") for a in args):
            raise Block("rg --pre executes a program")
        if name == "diff" and any(a.startswith("--to-file") for a in args):
            raise Block("unsupported diff option")
        return
    if name == "git":
        sub, rest = git_subcommand(words[1:])
        if sub is None:
            return
        rest_text = [w.text for w in rest]
        if any(a.startswith("--output") or a in ("--ext-diff", "--open-files-in-pager") for a in rest_text):
            raise Block("git option writes files or runs a program")
        if any(w.text == "-c" for w in words[1:2]):
            raise Block("git -c is not allowed in this profile")
        if sub.text in GIT_READONLY:
            return
        if sub.text == "branch" and all(a in GIT_BRANCH_READ for a in rest_text):
            return
        if sub.text == "config" and rest_text[:1] in (["--get"], ["--list"], ["-l"], ["--get-regexp"]):
            return
        raise Block(f"git {sub.text} is not in the {profile} allow-list")
    if profile == "verify":
        sub = args[0] if args else ""
        if name == "pnpm" and (sub in PNPM_VERIFY or (sub == "run" and len(args) > 1 and args[1] in PNPM_VERIFY)):
            return
        if name == "cargo" and (sub in CARGO_VERIFY or (sub == "fmt" and "--check" in args)):
            return
        if name == "actionlint":
            return
        if name == "python3" and (args[:3] == ["-B", "-m", "unittest"] or args[:2] == ["-m", "json.tool"]):
            return
        if name in ("node", "pnpm", "cargo", "rustc", "python3", "git", "gh") and args == ["--version"]:
            return
    raise Block(f"{name!r} is not in the {profile} allow-list")


def check_command(command, profile, cwd):
    for cmd in analyze_all(command):
        check_simple(cmd, profile, cwd)
    # Heredoc bodies fed to non-shell commands are plain data and are ignored on purpose.


def main(argv):
    profile = "project"
    if "--profile" in argv:
        profile = argv[argv.index("--profile") + 1]
    if profile not in ("project", "readonly", "verify"):
        raise Block(f"unknown guard profile {profile!r}")
    payload = json.load(sys.stdin)
    if not isinstance(payload, dict):
        raise Block("unexpected hook payload")
    tool = payload.get("tool_name")
    if tool not in (None, "Bash"):
        return
    tool_input = payload.get("tool_input")
    command = tool_input.get("command") if isinstance(tool_input, dict) else None
    if not isinstance(command, str) or not command.strip():
        raise Block("missing Bash command")
    check_command(command, profile, payload.get("cwd") or os.environ.get("CLAUDE_PROJECT_DIR"))


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except Block as exc:
        print(f"Blocked by guard_bash: {exc}", file=sys.stderr)
        sys.exit(2)
    except BaseException as exc:  # noqa: BLE001 - fail closed on anything unexpected
        print(f"Blocked by guard_bash: internal error ({type(exc).__name__}: {exc}); refusing to run", file=sys.stderr)
        sys.exit(2)
    sys.exit(0)
