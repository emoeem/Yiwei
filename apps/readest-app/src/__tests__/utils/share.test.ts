import { describe, it, expect } from 'vitest';
import { buildShareUrl, parseShareDeepLink } from '@/utils/share';
import { READEST_WEB_BASE_URL, SHARE_BASE_URL } from '@/services/constants';

// Derived from the configured base URL so the fork's own domain (or a
// self-hosted deployment) does not have to be hardcoded in the expectations.
const WEB_HOST = new URL(READEST_WEB_BASE_URL).host;
const PREVIEW_HOST = `staging.${WEB_HOST.slice(WEB_HOST.indexOf('.') + 1)}`;

describe('buildShareUrl', () => {
  it('builds the canonical https URL for a token', () => {
    expect(buildShareUrl('aBcDeFgHiJkLmNoPqRsTuV')).toBe(
      `${SHARE_BASE_URL}/aBcDeFgHiJkLmNoPqRsTuV`,
    );
  });
});

describe('parseShareDeepLink', () => {
  const VALID_TOKEN = 'aBcDeFgHiJkLmNoPqRsTuV';

  it('parses yiwei://share/{token}', () => {
    expect(parseShareDeepLink(`yiwei://share/${VALID_TOKEN}`)).toEqual({ token: VALID_TOKEN });
  });

  it('parses https://<web host>/s/{token}', () => {
    expect(parseShareDeepLink(`https://${WEB_HOST}/s/${VALID_TOKEN}`)).toEqual({
      token: VALID_TOKEN,
    });
  });

  it('parses subdomains of the configured host for preview deploys', () => {
    expect(parseShareDeepLink(`https://${PREVIEW_HOST}/s/${VALID_TOKEN}`)).toEqual({
      token: VALID_TOKEN,
    });
  });

  it('rejects tokens of the wrong length', () => {
    expect(parseShareDeepLink('yiwei://share/short')).toBeNull();
    expect(parseShareDeepLink(`yiwei://share/${VALID_TOKEN}extra`)).toBeNull();
  });

  it('rejects tokens with disallowed characters', () => {
    // Underscore and hyphen are explicitly NOT in the alphabet.
    const bad = 'aBcDeFgHiJkLmNoPqRsTu-';
    expect(parseShareDeepLink(`yiwei://share/${bad}`)).toBeNull();
  });

  it('rejects URLs from third-party hosts', () => {
    expect(parseShareDeepLink(`https://evil.example.com/s/${VALID_TOKEN}`)).toBeNull();
  });

  it('rejects yiwei:// URLs whose host is not "share"', () => {
    expect(parseShareDeepLink(`yiwei://book/${VALID_TOKEN}`)).toBeNull();
    expect(parseShareDeepLink(`yiwei://annotation/${VALID_TOKEN}`)).toBeNull();
  });

  it('rejects nested or extra path segments', () => {
    expect(parseShareDeepLink(`https://${WEB_HOST}/s/${VALID_TOKEN}/extra`)).toBeNull();
    expect(parseShareDeepLink(`https://${WEB_HOST}/extra/s/${VALID_TOKEN}`)).toBeNull();
  });

  it('returns null for malformed input', () => {
    expect(parseShareDeepLink('')).toBeNull();
    expect(parseShareDeepLink('not-a-url')).toBeNull();
    expect(parseShareDeepLink(`ftp://${WEB_HOST}/s/${VALID_TOKEN}`)).toBeNull();
  });
});
