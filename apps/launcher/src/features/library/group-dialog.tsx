import { Button, Dialog, Field, Input } from "@pl/ui";
import { type FormEvent, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { InstanceSummary } from "@/lib/api";

const MAX_GROUP = 40;

/** "Gruba taşı": type a group name or pick an existing one; empty means no group. */
export function GroupDialog({
  instance,
  groups,
  onClose,
  onSave,
}: {
  instance: InstanceSummary | null;
  /** Existing group names, for one-click picking. */
  groups: string[];
  onClose: () => void;
  onSave: (instance: InstanceSummary, group: string | null) => void;
}) {
  const { t } = useTranslation();
  const inputId = useId();
  const [value, setValue] = useState("");
  useEffect(() => {
    if (instance) setValue(instance.group ?? "");
  }, [instance]);

  const save = (group: string | null) => {
    if (!instance) return;
    const clean = group?.replace(/\s+/g, " ").trim() ?? "";
    onSave(instance, clean ? clean : null);
    onClose();
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    save(value);
  };

  return (
    <Dialog
      open={Boolean(instance)}
      onOpenChange={(open) => !open && onClose()}
      title={t("library.groups.dialogTitle")}
      description={t("library.groups.dialogBody", { name: instance?.name ?? "" })}
      closeLabel={t("common.close")}
      width={440}
      // A typed group name is saved as a whole: a stray click beside the dialog keeps it.
      dismissOnOutsideClick={false}
      footer={
        <>
          {instance?.group ? (
            <Button variant="ghost" className="mr-auto" onClick={() => save(null)}>
              {t("library.groups.remove")}
            </Button>
          ) : null}
          <Button variant="ghost" onClick={onClose}>
            {t("instance.cancel")}
          </Button>
          <Button variant="primary" type="submit" form={`${inputId}-form`}>
            {t("library.groups.save")}
          </Button>
        </>
      }
    >
      <form id={`${inputId}-form`} onSubmit={submit} className="space-y-3">
        <Field label={t("library.groups.name")} htmlFor={inputId} help={t("library.groups.help")}>
          <Input
            id={inputId}
            value={value}
            maxLength={MAX_GROUP}
            autoFocus
            placeholder={t("library.groups.placeholder")}
            onChange={(e) => setValue(e.target.value)}
          />
        </Field>
        {groups.length > 0 ? (
          <div className="flex flex-wrap gap-1.5">
            {groups.map((group) => (
              <Button
                key={group}
                size="sm"
                variant={group === value.trim() ? "primary" : "secondary"}
                onClick={() => setValue(group)}
              >
                {group}
              </Button>
            ))}
          </div>
        ) : null}
      </form>
    </Dialog>
  );
}
