import { DownloadSimple, Plus } from "@phosphor-icons/react";
import { Button, Dialog, EmptyState, SearchInput, Skeleton, toast } from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import emptyArt from "@/assets/art/empty-library.webp";
import { api, type InstanceSummary } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { CreateInstanceDialog } from "./create-instance-dialog";
import { type InstanceActions, InstanceCard } from "./instance-card";

function Section({
  title,
  items,
  selectedId,
  actions,
  offset,
}: {
  title: string;
  items: InstanceSummary[];
  selectedId: string | undefined;
  actions: InstanceActions;
  offset: number;
}) {
  if (!items.length) return null;
  return (
    <section className="mb-8">
      <h2 className="mb-3 font-display text-[16px] font-semibold text-ash-300">{title}</h2>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-4">
        {items.map((instance, i) => (
          <InstanceCard
            key={instance.id}
            instance={instance}
            selected={instance.id === selectedId}
            actions={actions}
            index={offset + i}
          />
        ))}
      </div>
    </section>
  );
}

export function LibraryPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const search = useSearch({ strict: false }) as { create?: boolean };
  const [query, setQuery] = useState("");
  const [pendingDelete, setPendingDelete] = useState<InstanceSummary | null>(null);
  const { data: instances, isPending } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const selectedId = usePrefs((s) => s.selectedInstanceId) ?? instances?.[0]?.id;
  const selectInstance = usePrefs((s) => s.selectInstance);
  const play = useLaunch((s) => s.play);

  const createOpen = Boolean(search.create);
  const setCreateOpen = (open: boolean) =>
    void navigate({ to: "/library", search: open ? { create: true } : {} });

  const filtered = useMemo(() => {
    const q = query.trim().toLocaleLowerCase("tr");
    return (instances ?? []).filter(
      (i) => !q || i.name.toLocaleLowerCase("tr").includes(q) || i.mcVersion.includes(q),
    );
  }, [instances, query]);

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ["instances"] });
  const duplicate = useMutation({
    mutationFn: (i: InstanceSummary) => api.duplicateInstance(i.id),
    onSuccess: (copy) => {
      invalidate();
      toast.success(t("library.duplicated", { name: copy.name }));
    },
  });
  const remove = useMutation({
    mutationFn: (i: InstanceSummary) => api.deleteInstance(i.id),
    onSuccess: (_, i) => {
      invalidate();
      toast(t("library.deleted", { name: i.name }));
    },
  });

  const actions: InstanceActions = {
    play: (i) => {
      selectInstance(i.id);
      void play(i.id);
    },
    openFolder: (i) => void api.openInstanceFolder(i.id),
    duplicate: (i) => duplicate.mutate(i),
    exportPack: () => toast(t("common.comingSoon")),
    remove: (i) => setPendingDelete(i),
  };

  const official = filtered.filter((i) => i.managed);
  const mine = filtered.filter((i) => !i.managed);

  return (
    <div className="flex min-h-0 flex-1 flex-col px-10 pt-4">
      <header className="flex flex-wrap items-end justify-between gap-4 pb-6">
        <div>
          <h1 className="font-display text-[32px] leading-tight font-semibold text-bone-50">
            {t("library.title")}
          </h1>
          <p className="mt-1 text-ash-400">
            {instances ? t("library.count", { count: instances.length }) : t("common.loading")}
          </p>
        </div>
        <div className="flex items-center gap-2.5">
          <SearchInput
            value={query}
            onChange={setQuery}
            placeholder={t("library.search")}
            label={t("library.search")}
            className="w-64"
          />
          <Button
            variant="ghost"
            icon={<DownloadSimple size={17} />}
            onClick={() => setCreateOpen(true)}
          >
            {t("library.import")}
          </Button>
          <Button
            variant="secondary"
            icon={<Plus size={17} weight="bold" className="text-gold-300" />}
            onClick={() => setCreateOpen(true)}
          >
            {t("library.create")}
          </Button>
        </div>
      </header>

      <div className="-mx-2 min-h-0 flex-1 overflow-y-auto px-2 pb-6">
        {isPending ? (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(230px,1fr))] gap-4">
            {[0, 1, 2, 3].map((i) => (
              <Skeleton key={i} className="h-[220px] rounded-[var(--radius-panel)]" />
            ))}
          </div>
        ) : instances && instances.length === 0 ? (
          <EmptyState
            illustration={emptyArt}
            title={t("library.empty.title")}
            body={t("library.empty.body")}
            action={
              <Button
                variant="primary"
                icon={<Plus size={16} weight="bold" />}
                onClick={() => setCreateOpen(true)}
              >
                {t("library.empty.action")}
              </Button>
            }
          />
        ) : filtered.length === 0 ? (
          <p className="pt-10 text-center text-ash-400">{t("library.noResults")}</p>
        ) : (
          <>
            <Section
              title={t("library.official")}
              items={official}
              selectedId={selectedId}
              actions={actions}
              offset={0}
            />
            <Section
              title={t("library.mine")}
              items={mine}
              selectedId={selectedId}
              actions={actions}
              offset={official.length}
            />
          </>
        )}
      </div>

      <CreateInstanceDialog open={createOpen} onOpenChange={setCreateOpen} />
      <Dialog
        open={Boolean(pendingDelete)}
        onOpenChange={(open) => !open && setPendingDelete(null)}
        title={t("library.deleteConfirm.title", { name: pendingDelete?.name ?? "" })}
        description={t("library.deleteConfirm.body")}
        closeLabel={t("common.close")}
        width={440}
        footer={
          <>
            <Button variant="ghost" onClick={() => setPendingDelete(null)}>
              {t("create.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                if (pendingDelete) remove.mutate(pendingDelete);
                setPendingDelete(null);
              }}
            >
              {t("library.deleteConfirm.confirm")}
            </Button>
          </>
        }
      />
    </div>
  );
}
