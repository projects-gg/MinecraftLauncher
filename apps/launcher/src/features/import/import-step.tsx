import { DownloadSimple } from "@phosphor-icons/react";
import { Button } from "@pl/ui";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ImportScanView } from "@/lib/api";
import { ImportPanel, ImportTotals } from "./import-choices";
import { useImportJob } from "./import-job";
import { ImportStatusCard } from "./import-status";
import { useImportPicker } from "./use-import-picker";

/**
 * The welcome flow's "Başka bir launcher'dan geldiğini gördük" step, under its title: the rows,
 * the selected size against the free space, "Aktar" (the import goes on in the background and
 * the flow moves on) and "Aktarmadan devam et". Coming back to the step while it runs shows its
 * progress instead.
 */
export function ImportStep({
  scan,
  back,
  onNext,
}: {
  scan: ImportScanView;
  /** The flow's own "Geri". */
  back: ReactNode;
  onNext: () => void;
}) {
  const { t } = useTranslation();
  const picker = useImportPicker(scan);
  const status = useImportJob((s) => s.status);
  const start = useImportJob((s) => s.start);

  if (status !== "idle") {
    return (
      <>
        <ImportStatusCard className="mt-7" />
        <div className="mt-9 flex flex-wrap items-center gap-3">
          <Button variant="primary" size="lg" onClick={onNext}>
            {t("onboarding.account.continue")}
          </Button>
          {back}
        </div>
      </>
    );
  }

  return (
    <>
      <ImportPanel picker={picker} className="mt-6" />
      <ImportTotals picker={picker} className="mt-2.5" />
      <div className="mt-7 flex flex-wrap items-center gap-3">
        <Button
          variant="primary"
          size="lg"
          icon={<DownloadSimple size={17} weight="bold" />}
          disabled={picker.empty || !picker.enough || picker.pending}
          onClick={() => {
            start(picker.request);
            onNext();
          }}
        >
          {t("import.step.import")}
        </Button>
        <Button variant="secondary" onClick={onNext}>
          {t("import.step.skip")}
        </Button>
        {back}
      </div>
    </>
  );
}
