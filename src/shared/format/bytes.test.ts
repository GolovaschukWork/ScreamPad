import { describe, expect, it } from "vitest";
import { formatBytes } from "./bytes";

describe("formatBytes", () => {
  it("scales by powers of 1024 with at most one decimal", () => {
    expect(formatBytes(0, "en")).toBe("0 byte");
    expect(formatBytes(512, "en")).toBe("512 byte");
    expect(formatBytes(1024, "en")).toBe("1 kB");
    expect(formatBytes(125_000, "en")).toBe("122.1 kB");
    expect(formatBytes(5 * 1024 * 1024, "en")).toBe("5 MB");
  });

  it("uses the locale's number format", () => {
    expect(formatBytes(125_000, "uk")).toMatch(/^122,1\s/);
  });

  it("never shows a negative or non-finite size", () => {
    expect(formatBytes(-5, "en")).toBe("0 byte");
    expect(formatBytes(Number.NaN, "en")).toBe("0 byte");
    expect(formatBytes(Number.POSITIVE_INFINITY, "en")).toBe("0 byte");
  });
});
