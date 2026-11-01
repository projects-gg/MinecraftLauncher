import { type MarkSize, ProfileMark } from "./profile-mark";

export type { MarkSize } from "./profile-mark";

/**
 * Which mark: `verified` ("Doğrulanmış hesap", blue) or `admin` ("Projects yöneticisi", red), as a
 * card's legacy `mark` names it (SOC-VERIFY). Since BADGES both are badges of the catalogue
 * (`ProfileMark`); this keeps SOC-VERIFY's callers as they were.
 */
export type MarkKind = "verified" | "admin";

/**
 * SOC-VERIFY's mark: a blue or red seal with a white check right of a person's name, the same
 * wherever the person shows. `ProfileMark` draws it (docs/design.md §6, "Profile marks").
 */
export function VerifiedMark({
  kind,
  size = 14,
  label,
  tooltip = true,
  className,
}: {
  kind: MarkKind;
  size?: MarkSize;
  /** "Doğrulanmış hesap" / "Projects yöneticisi" in the interface's language. */
  label: string;
  tooltip?: boolean;
  className?: string;
}) {
  return (
    <ProfileMark
      color={kind === "admin" ? "red" : "blue"}
      glyph="check"
      shape="seal"
      size={size}
      label={label}
      tooltip={tooltip}
      className={className}
    />
  );
}
