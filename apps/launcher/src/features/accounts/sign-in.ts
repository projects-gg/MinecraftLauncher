/**
 * Errors only a new Microsoft sign-in fixes: an expired or revoked session (`MSA-…`, Microsoft's
 * own reason after the dash) or its secrets gone from this computer (`KC-YOK`). Their toasts offer
 * "Yeniden giriş yap" for the account.
 */
export function needsSignIn(code: string): boolean {
  return code.startsWith("MSA-") || code === "KC-YOK";
}
