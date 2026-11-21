const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/**
 * Binary-prefixed size: one decimal under 100 ("1.5 KB", "6.2 GB"), none
 * from 100 up ("140 MB"), and never a ".0" ("500 MB", "1 GB").
 */
export function formatBytes(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  if (n < 1024) return `${Math.round(n)} B`;
  let value = n;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const text = value >= 100 ? value.toFixed(0) : value.toFixed(1).replace(/\.0$/, "");
  return `${text} ${UNITS[unit]}`;
}
