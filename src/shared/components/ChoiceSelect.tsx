import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/shared/ui/select";

export type ChoiceOption = { value: string; label: string };

/** A select over a short fixed list of labelled choices. Name it with `id` (and a `<label htmlFor>`) or with `ariaLabel`. */
export function ChoiceSelect({
  id,
  ariaLabel,
  value,
  options,
  onChange,
  className,
  disabled,
}: {
  id?: string;
  ariaLabel?: string;
  value: string;
  options: ChoiceOption[];
  onChange: (value: string) => void;
  className?: string;
  disabled?: boolean;
}) {
  return (
    <Select
      disabled={disabled}
      value={value}
      items={options}
      onValueChange={(next) => {
        if (typeof next === "string") onChange(next);
      }}
    >
      <SelectTrigger id={id} aria-label={ariaLabel} className={className ?? "min-w-44"}>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {options.map((option) => (
          <SelectItem key={option.value} value={option.value}>
            {option.label}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
