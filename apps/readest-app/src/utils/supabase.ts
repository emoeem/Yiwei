import { createClient } from '@supabase/supabase-js';
import { getRuntimeConfig } from '@/services/runtimeConfig';
import { decodeEnvBase64 } from '@/utils/envBase64';

const supabaseUrl =
  getRuntimeConfig()?.supabaseUrl ||
  process.env['SUPABASE_URL'] ||
  process.env['NEXT_PUBLIC_SUPABASE_URL'] ||
  decodeEnvBase64(process.env['NEXT_PUBLIC_DEFAULT_SUPABASE_URL_BASE64']);
const supabaseAnonKey =
  getRuntimeConfig()?.supabaseAnonKey ||
  process.env['SUPABASE_ANON_KEY'] ||
  process.env['NEXT_PUBLIC_SUPABASE_ANON_KEY'] ||
  decodeEnvBase64(process.env['NEXT_PUBLIC_DEFAULT_SUPABASE_KEY_BASE64']);

/**
 * Whether this build has a cloud backend at all (own Supabase project, or a
 * self-hosted deployment). False means the app is local-only: cloud UI must
 * either hide itself or fail with a message rather than reaching anyone else's
 * server.
 */
export const isCloudConfigured = (): boolean => Boolean(supabaseUrl && supabaseAnonKey);

// RFC 2606 reserves `.invalid`, so an unconfigured build cannot reach a real
// backend even if a cloud code path is invoked. The client has to be
// constructed eagerly because this module is imported at app boot; only the
// calls are expected to fail.
const UNCONFIGURED_URL = 'https://unconfigured.invalid';
const UNCONFIGURED_KEY = 'unconfigured';

const resolvedSupabaseUrl = supabaseUrl || UNCONFIGURED_URL;
const resolvedSupabaseAnonKey = supabaseAnonKey || UNCONFIGURED_KEY;

export const supabase = createClient(resolvedSupabaseUrl, resolvedSupabaseAnonKey);

export const createSupabaseClient = (accessToken?: string) => {
  return createClient(resolvedSupabaseUrl, resolvedSupabaseAnonKey, {
    global: {
      headers: accessToken
        ? {
            Authorization: `Bearer ${accessToken}`,
          }
        : {},
    },
  });
};

export const createSupabaseAdminClient = () => {
  const supabaseAdminKey = process.env['SUPABASE_ADMIN_KEY'] || '';
  return createClient(resolvedSupabaseUrl, supabaseAdminKey || UNCONFIGURED_KEY, {
    auth: {
      persistSession: false,
      autoRefreshToken: false,
      detectSessionInUrl: false,
    },
  });
};
