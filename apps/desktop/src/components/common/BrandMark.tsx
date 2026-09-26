import { GraduationCap } from "lucide-react";
import { brand } from "@/brand";
import { cn } from "@/lib/utils";

/** Logo + product name from the brand config. Falls back to a neutral icon. */
export function BrandMark({
  className,
  showName = true,
}: {
  className?: string;
  showName?: boolean;
}) {
  return (
    <span className={cn("inline-flex items-center gap-2", className)}>
      {brand.logo ? (
        <picture>
          {brand.logo.dark ? (
            <source srcSet={brand.logo.dark} media="(prefers-color-scheme: dark)" />
          ) : null}
          <img src={brand.logo.light} alt="" className="size-7" />
        </picture>
      ) : (
        <span className="grid size-7 place-items-center rounded-lg bg-primary text-primary-foreground">
          <GraduationCap className="size-4" aria-hidden />
        </span>
      )}
      {showName ? (
        <span className="font-heading font-semibold tracking-tight">{brand.productName}</span>
      ) : null}
    </span>
  );
}
