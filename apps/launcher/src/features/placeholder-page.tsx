import { useTranslation } from "react-i18next";

/** Temporary page body for sections still being built. */
export function PlaceholderPage({ title }: { title: string }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 flex-col px-page pt-3">
      <h1 className="page-title">{title}</h1>
      <p className="mt-2 text-ash-400">{t("common.comingSoon")}</p>
    </div>
  );
}
