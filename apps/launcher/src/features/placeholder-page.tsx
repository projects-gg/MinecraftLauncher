import { useTranslation } from "react-i18next";

/** Temporary page body for sections still being built. */
export function PlaceholderPage({ title }: { title: string }) {
  const { t } = useTranslation();
  return (
    <div className="flex flex-1 flex-col px-10 pt-6">
      <h1 className="font-display text-[32px] leading-tight font-semibold text-bone-50">{title}</h1>
      <p className="mt-2 text-ash-400">{t("common.comingSoon")}</p>
    </div>
  );
}
