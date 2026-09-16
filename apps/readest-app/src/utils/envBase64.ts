/**
 * Decode a base64-encoded environment default.
 *
 * Upstream bakes its own service credentials into the build as base64 via
 * `NEXT_PUBLIC_DEFAULT_*_BASE64`. A fork ships without them, and two of these
 * decodes run at module load (`utils/supabase.ts`, `context/PHContext.tsx`), so
 * a missing value would throw from `atob` and take the whole app down instead of
 * just disabling the optional cloud feature. Everything that consumes these
 * variables treats "absent or malformed" as "feature not configured".
 */
export const decodeEnvBase64 = (value: string | undefined): string => {
  if (!value) return '';
  try {
    return atob(value);
  } catch {
    return '';
  }
};
