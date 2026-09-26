import { cn } from "cn";
import { Loader2Icon } from "lucide-react";

/**
 * App-specific: decorative (aria-hidden) by default, because it usually sits next to text
 * that already says what is happening ("Saving…"). Pass `aria-label` to make it a
 * standalone status announcement.
 */
function Spinner({ className, ...props }: React.ComponentProps<"svg">) {
  const labelled = typeof props["aria-label"] === "string";
  return (
    <Loader2Icon
      data-slot="spinner"
      role={labelled ? "status" : undefined}
      aria-hidden={labelled ? undefined : true}
      className={cn("size-4 animate-spin", className)}
      {...props}
    />
  );
}

export { Spinner };
