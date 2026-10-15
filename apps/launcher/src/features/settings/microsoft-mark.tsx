/** Microsoft's four-square mark, as required on "Sign in with Microsoft" buttons. */
export function MicrosoftMark({ size = 16 }: { size?: number }) {
  const cell = (size - 1) / 2;
  return (
    <span
      aria-hidden
      className="grid shrink-0 grid-cols-2 gap-px"
      style={{ width: size, height: size }}
    >
      {["#F25022", "#7FBA00", "#00A4EF", "#FFB900"].map((color) => (
        <span key={color} style={{ width: cell, height: cell, background: color }} />
      ))}
    </span>
  );
}
