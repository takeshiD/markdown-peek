// Notifications (bottom right): settings that cannot work, failed runs,
// "saved as default". Errors stay until closed; info fades after a while.
import { AlertTriangle, CheckCircle2, X } from "lucide-react";
import { cn } from "cn";
import { Button } from "@/components/ui/button";
import { useStore } from "../lib/store";
import { dismiss } from "../lib/actions";

export function Toaster() {
  const toasts = useStore((s) => s.toasts);
  return (
    <div className="pointer-events-none fixed right-4 bottom-4 z-50 flex w-[min(380px,calc(100vw-32px))] flex-col gap-2" data-testid="toaster">
      {toasts.map((t) => (
        <div
          key={t.id}
          role={t.kind === "error" ? "alert" : "status"}
          className={cn(
            "mp-toast pointer-events-auto flex gap-3 rounded-lg border bg-popover p-3 text-[13px] text-popover-foreground shadow-lg",
            t.kind === "error" && "border-danger/50",
          )}
          data-testid="toast"
          data-kind={t.kind}
        >
          {t.kind === "error" ? (
            <AlertTriangle className="mt-0.5 size-4 shrink-0 text-danger" aria-hidden="true" />
          ) : (
            <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-ok" aria-hidden="true" />
          )}
          <div className="min-w-0 flex-1">
            <p className="font-semibold">{t.title}</p>
            {t.body.map((line, i) => (
              <p key={i} className="mt-0.5 break-words text-muted-foreground">
                {line}
              </p>
            ))}
            {t.action && (
              <Button
                variant="outline"
                size="sm"
                className="mt-2 h-7 max-md:h-11"
                onClick={() => {
                  dismiss(t.id);
                  t.action!.run();
                }}
              >
                {t.action.label}
              </Button>
            )}
          </div>
          <Button variant="ghost" size="icon-sm" className="-mt-1 -mr-1 size-7 shrink-0 text-muted-foreground max-md:size-11" aria-label="Dismiss notification" onClick={() => dismiss(t.id)}>
            <X />
          </Button>
        </div>
      ))}
    </div>
  );
}
