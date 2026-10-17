import { Copy, WarningCircle } from "@phosphor-icons/react";
import { Button, cn, toast } from "@pl/ui";
import { useTranslation } from "react-i18next";
import { copyText } from "@/lib/clipboard";

/** A failed command as the UI shows it: the friendly message, details only for "copy". */
export interface Failure {
  code: string;
  message: string;
  details: string;
}

export function failure(error: unknown, fallback: string): Failure {
  if (error instanceof Error && "code" in error && "details" in error) {
    const code = String(error.code);
    return {
      code,
      message: error.message || fallback,
      details: `${code}\n${String(error.details)}`.trim(),
    };
  }
  // IPC failures and programming errors are not meant for the player's eyes.
  return {
    code: "",
    message: fallback,
    details: error instanceof Error ? error.message : String(error),
  };
}

export function ErrorLine({ error, className }: { error: Failure; className?: string }) {
  const { t } = useTranslation();
  return (
    <div role="alert" className={cn("flex items-start gap-2.5", className)}>
      <WarningCircle size={17} weight="fill" className="mt-px shrink-0 text-flare-400" />
      <p className="min-w-0 flex-1 text-[13px] text-flare-400">{error.message}</p>
      <Button
        size="sm"
        variant="ghost"
        icon={<Copy size={14} />}
        className="-my-1.5"
        onClick={() =>
          void copyText(error.details).then(
            () => toast.success(t("common.detailsCopied")),
            () => toast.error(t("common.copyFailed")),
          )
        }
      >
        {t("common.copyDetails")}
      </Button>
    </div>
  );
}
