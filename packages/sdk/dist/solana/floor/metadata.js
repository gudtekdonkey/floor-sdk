

import { addressBytes, base58Encode, hex } from "../../codec.js";
import { sha256 } from "../../sha256.js";
import { launchPda } from "./program.js";
import { metadataUri } from "./instructions.js";

export const FLOOR_ORIGIN = "https://floor.supply";

export const WALLET_AUTH_PREFIX = "FLOOR metadata";
export const WALLET_AUTH_MESSAGE_HEADER = "x-floor-auth-message";
export const WALLET_AUTH_SIGNATURE_HEADER = "x-floor-auth-signature";

export const WALLET_AUTH_MAX_AHEAD_S = 300;

export const WALLET_AUTH_TTL_S = 120;

export const MAX_DESCRIPTION = 2000;
export const MAX_URL = 300;
export const MAX_HANDLE = 64;
export const MAX_IMAGE_BYTES = { avatar: 1024 * 1024, banner: 2 * 1024 * 1024 }         ;
export const SOCIAL_KEYS = ["x", "telegram", "discord", "github"]         ;

export class LaunchMetadataError extends Error {
           code        ;
           status                    ;
  constructor(code        , message        , status         ) {
    super(message);
    this.name = "LaunchMetadataError";
    this.code = code;
    this.status = status;
  }
}

export { launchPda, metadataUri };

export const metadataBase = (cluster               , origin = FLOOR_ORIGIN)         =>
  `${origin.replace(/\/$/, "")}/api/meta/solana/${cluster}`;

export const chainParam = (cluster               )         => `solana-${cluster}`;

const SITE = /^(?=.{1,253}(?::|$))[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)*(?::[1-9][0-9]{0,4})?$/;

function assertAddress(field        , a        )       {
  let ok = false;
  try {
    ok = addressBytes(a).length === 32 && base58Encode(addressBytes(a)) === a;
  } catch {
    ok = false;
  }
  if (!ok) throw new LaunchMetadataError("BadAddress", `${field} is not a base58 address in its canonical spelling`);
}

export function siteOf(origin        )         {
  const site = new URL(origin).host.toLowerCase();
  const port = site.split(":")[1];
  if (!SITE.test(site) || (port !== undefined && Number(port) > 65535)) {
    throw new LaunchMetadataError("BadSite", `${origin} has no host the server can name`);
  }
  return site;
}

export function walletAuthMessage(f

 )         {
  if (!SITE.test(f.site)) throw new LaunchMetadataError("BadSite", `site ${f.site} is not a lowercase host[:port]`);
  assertAddress("launch", f.launch);
  assertAddress("signer", f.signer);
  if (!/^[0-9a-f]{64}$/.test(f.bodySha256)) throw new LaunchMetadataError("BadHash", "bodySha256 is not 64 lowercase hex");
  if (!Number.isSafeInteger(f.expires) || !/^[1-9][0-9]{0,10}$/.test(String(f.expires))) {
    throw new LaunchMetadataError("BadExpiry", "expires is not unix seconds");
  }
  return [
    WALLET_AUTH_PREFIX,
    `site: ${f.site}`,
    `action: ${f.action}`,
    `chain: ${chainParam(f.cluster)}`,
    `launch: ${f.launch}`,
    `signer: ${f.signer}`,
    `body-sha256: ${f.bodySha256}`,
    `expires: ${f.expires}`,
  ].join("\n");
}

export function walletAuthHeaders(message        , signature            )                         {
  if (signature.length !== 64) throw new LaunchMetadataError("BadSignature", "an ed25519 signature is 64 bytes");
  return { [WALLET_AUTH_MESSAGE_HEADER]: btoa(message), [WALLET_AUTH_SIGNATURE_HEADER]: base58Encode(signature) };
}

const ascii = (s        )             => new TextEncoder().encode(s);

export function sniffImageType(b            )                   {
  const at = (sig          , off = 0) => sig.every((x, i) => b[off + i] === x);
  if (at([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return "image/png";
  if (at([0xff, 0xd8, 0xff])) return "image/jpeg";
  if (at([0x52, 0x49, 0x46, 0x46]) && at([0x57, 0x45, 0x42, 0x50], 8)) return "image/webp";
  return null;
}

export function encodeUploadBody(f                                                                                , boundary        )

  {
  const type = sniffImageType(f.image);
  if (type === null) throw new LaunchMetadataError("BadImage", `the ${f.kind} is not a PNG, JPEG or WebP`);
  if (f.image.length === 0 || f.image.length > MAX_IMAGE_BYTES[f.kind]) {
    throw new LaunchMetadataError("ImageTooLarge", `the ${f.kind} is at most ${MAX_IMAGE_BYTES[f.kind]} bytes`);
  }
  if (!/^[0-9A-Za-z-]{16,70}$/.test(boundary)) throw new LaunchMetadataError("BadBoundary", "boundary must be 16-70 of [0-9A-Za-z-]");
  const field = (name        , value        ) =>
    `--${boundary}\r\nContent-Disposition: form-data; name="${name}"\r\n\r\n${value}\r\n`;
  const ext = type === "image/jpeg" ? "jpg" : type.slice(6);
  const head = ascii(
    field("kind", f.kind) + field("launchAddress", f.launch) + field("chainId", chainParam(f.cluster)) +
      `--${boundary}\r\nContent-Disposition: form-data; name="file"; filename="${f.kind}.${ext}"\r\nContent-Type: ${type}\r\n\r\n`,
  );
  const tail = ascii(`\r\n--${boundary}--\r\n`);
  const body = new Uint8Array(head.length + f.image.length + tail.length);
  body.set(head, 0);
  body.set(f.image, head.length);
  body.set(tail, head.length + f.image.length);
  return { body, contentType: `multipart/form-data; boundary=${boundary}` };
}

                                                          

export function encodeLaunchMetaBody(m            )             {
  const description = m.description ?? null;
  if (description !== null && description.length > MAX_DESCRIPTION) {
    throw new LaunchMetadataError("BadDescription", `description is at most ${MAX_DESCRIPTION} characters`);
  }
  const website = m.website ?? null;
  if (website !== null) {
    if (website.length > MAX_URL) throw new LaunchMetadataError("BadWebsite", "website is too long");
    let ok = /^https?:\/\//i.test(website);
    try { new URL(website); } catch { ok = false; }
    if (!ok) throw new LaunchMetadataError("BadWebsite", "website must be an http(s) URL");
  }
  const socials                                  = {};
  for (const k of SOCIAL_KEYS) {
    const v = m.socials?.[k];
    if (v == null || v === "") continue;
    const handle = v.trim().replace(/^@/, "").toLowerCase();
    if (handle.length > MAX_HANDLE) throw new LaunchMetadataError("BadSocial", `socials.${k} is at most ${MAX_HANDLE} characters`);
    socials[k] = handle;
  }
  return ascii(JSON.stringify({
    description, website, avatarKey: m.avatarKey ?? null, bannerKey: m.bannerKey ?? null, socials,
  }));
}

async function signedWrite(ctx              , action                  , method        , path        , body            , contentType        )                   {
  const origin = (ctx.origin ?? FLOOR_ORIGIN).replace(/\/$/, "");
  const now = ctx.now ?? (() => Math.floor(Date.now() / 1000));
  const message = walletAuthMessage({
    site: siteOf(origin), action, cluster: ctx.cluster, launch: ctx.launch, signer: ctx.signer.address,
    bodySha256: hex(sha256(body)), expires: now() + WALLET_AUTH_TTL_S,
  });
  const signature = await ctx.signer.signMessage(ascii(message));
  const f = ctx.fetch ?? (globalThis.fetch                    );
  const res = await f(`${origin}${path}`, {
    method, headers: { ...walletAuthHeaders(message, signature), "content-type": contentType }, body,
  });
  let json          = null;
  try { json = await res.json(); } catch { json = null; }
  if (res.status !== 200) {
    const error = (json                              )?.error;
    throw new LaunchMetadataError("Refused", typeof error === "string" ? error : `HTTP ${res.status}`, res.status);
  }
  return (json                               )?.result;
}

export async function uploadLaunchImage(ctx                                                       )                  {
  const boundary = `floor-${hex(sha256(ctx.image)).slice(0, 24)}`;
  const { body, contentType } = encodeUploadBody({ kind: ctx.kind, launch: ctx.launch, cluster: ctx.cluster, image: ctx.image }, boundary);
  const result = await signedWrite(ctx, "upload", "POST", "/api/uploads", body, contentType);
  const key = (result                                 )?.key;
  if (typeof key !== "string") throw new LaunchMetadataError("BadResponse", "the upload answered without a key");
  return key;
}

export function putLaunchMeta(ctx                                     )                   {
  return signedWrite(ctx, "meta", "PUT", `/api/meta/launches/${chainParam(ctx.cluster)}/${ctx.launch}`,
    encodeLaunchMetaBody(ctx.meta), "application/json");
}

export async function saveLaunchMetadata(input

 )                                                                                  {
  assertAddress("mint", input.mint);
  const launch = launchPda(input.mint, input.programId)[0];
  const ctx               = { cluster: input.cluster, launch, signer: input.signer, origin: input.origin, fetch: input.fetch, now: input.now };

  encodeLaunchMetaBody({ description: input.description, website: input.website, socials: input.socials });
  const avatarKey = input.avatar ? await uploadLaunchImage({ ...ctx, kind: "avatar", image: input.avatar }) : null;
  const bannerKey = input.banner ? await uploadLaunchImage({ ...ctx, kind: "banner", image: input.banner }) : null;
  await putLaunchMeta({ ...ctx, meta: { description: input.description, website: input.website, socials: input.socials, avatarKey, bannerKey } });
  return { launch, avatarKey, bannerKey };
}
