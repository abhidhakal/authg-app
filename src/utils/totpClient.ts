// Client-side TOTP and Google Authenticator Migration Parser

export interface OtpAccount {
  id: string;
  name: string;
  issuer: string;
  secret: string; // Base32
  algorithm: 'SHA1' | 'SHA256' | 'SHA512';
  digits: number;
  period: number;
  otpType: 'TOTP' | 'HOTP';
  lastUsed?: number; // ms timestamp of last copy, for recently-used ordering
}

export interface CodeInfo {
  code: string;
  secondsRemaining: number;
  period: number;
  progressPercent: number;
  nextCode?: string; // only near the end of a period
}

// RFC 4648 Base32 alphabet
const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

export function normalizeBase32(secret: string): string {
  return secret
    .replace(/[\s-]/g, '')
    .toUpperCase()
    .replace(/=+$/, '');
}

export function base32ToBytes(base32Str: string): Uint8Array {
  const clean = normalizeBase32(base32Str);
  let bits = 0;
  let value = 0;
  const bytes: number[] = [];

  for (let i = 0; i < clean.length; i++) {
    const char = clean[i];
    const val = BASE32_ALPHABET.indexOf(char);
    if (val === -1) {
      throw new Error(`Invalid Base32 character: ${char}`);
    }
    value = (value << 5) | val;
    bits += 5;
    if (bits >= 8) {
      bytes.push((value >>> (bits - 8)) & 255);
      bits -= 8;
    }
  }
  return new Uint8Array(bytes);
}

export function bytesToBase32(bytes: Uint8Array): string {
  let bits = 0;
  let value = 0;
  let output = '';

  for (let i = 0; i < bytes.length; i++) {
    value = (value << 8) | bytes[i];
    bits += 8;
    while (bits >= 5) {
      output += BASE32_ALPHABET[(value >>> (bits - 5)) & 31];
      bits -= 5;
    }
  }
  if (bits > 0) {
    output += BASE32_ALPHABET[(value << (5 - bits)) & 31];
  }
  return output;
}

// Compute standard RFC 6238 TOTP using Web Crypto API
export async function computeTotp(
  secretBase32: string,
  algorithm: 'SHA1' | 'SHA256' | 'SHA512' = 'SHA1',
  digits: number = 6,
  period: number = 30,
  nowSec: number = Math.floor(Date.now() / 1000)
): Promise<CodeInfo> {
  const keyBytes = base32ToBytes(secretBase32);
  const counter = Math.floor(nowSec / period);

  const counterBytes = new Uint8Array(8);
  let tempCounter = counter;
  for (let i = 7; i >= 0; i--) {
    counterBytes[i] = tempCounter & 0xff;
    tempCounter = Math.floor(tempCounter / 256);
  }

  const hashName = algorithm === 'SHA1' ? 'SHA-1' : algorithm === 'SHA256' ? 'SHA-256' : 'SHA-512';

  const cryptoKey = await window.crypto.subtle.importKey(
    'raw',
    keyBytes as unknown as BufferSource,
    { name: 'HMAC', hash: { name: hashName } },
    false,
    ['sign']
  );

  const signature = await window.crypto.subtle.sign('HMAC', cryptoKey, counterBytes);
  const hash = new Uint8Array(signature);

  const offset = hash[hash.length - 1] & 0x0f;
  const binaryCode =
    ((hash[offset] & 0x7f) << 24) |
    ((hash[offset + 1] & 0xff) << 16) |
    ((hash[offset + 2] & 0xff) << 8) |
    (hash[offset + 3] & 0xff);

  const divisor = Math.pow(10, digits);
  const otp = binaryCode % divisor;
  const code = otp.toString().padStart(digits, '0');

  const secondsRemaining = period - (nowSec % period);
  const progressPercent = (secondsRemaining / period) * 100;

  return {
    code,
    secondsRemaining,
    period,
    progressPercent,
  };
}

// Protobuf wire reader for Google Authenticator migration payload
class ProtobufReader {
  private buffer: Uint8Array;
  private pos = 0;

  constructor(buffer: Uint8Array) {
    this.buffer = buffer;
  }

  hasMore(): boolean {
    return this.pos < this.buffer.length;
  }

  readVarint(): number {
    let result = 0;
    let shift = 0;
    while (this.pos < this.buffer.length) {
      const b = this.buffer[this.pos++];
      result |= (b & 0x7f) << shift;
      if ((b & 0x80) === 0) break;
      shift += 7;
    }
    return result;
  }

  readBytes(len: number): Uint8Array {
    if (this.pos + len > this.buffer.length) {
      throw new Error('Protobuf unexpected end of buffer');
    }
    const slice = this.buffer.subarray(this.pos, this.pos + len);
    this.pos += len;
    return slice;
  }

  skip(wireType: number): void {
    if (wireType === 0) {
      this.readVarint();
    } else if (wireType === 1) {
      this.pos += 8;
    } else if (wireType === 2) {
      const len = this.readVarint();
      this.pos += len;
    } else if (wireType === 5) {
      this.pos += 4;
    } else {
      throw new Error(`Unsupported wire type: ${wireType}`);
    }
  }
}

export function parseGoogleMigrationUri(uri: string): OtpAccount[] {
  const trimmed = uri.trim();
  const dataIdx = trimmed.indexOf('data=');
  const rawData = dataIdx !== -1 ? trimmed.substring(dataIdx + 5).split('&')[0] : trimmed;
  const unescaped = decodeURIComponent(rawData);

  const binaryString = atob(unescaped);
  const bytes = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i);
  }

  const reader = new ProtobufReader(bytes);
  const accounts: OtpAccount[] = [];

  while (reader.hasMore()) {
    const tag = reader.readVarint();
    const fieldNum = tag >> 3;
    const wireType = tag & 0x07;

    if (fieldNum === 1 && wireType === 2) {
      const len = reader.readVarint();
      const subBytes = reader.readBytes(len);
      const acc = parseOtpParameter(subBytes);
      if (acc) accounts.push(acc);
    } else {
      reader.skip(wireType);
    }
  }

  return accounts;
}

function parseOtpParameter(bytes: Uint8Array): OtpAccount | null {
  const reader = new ProtobufReader(bytes);
  let secretBytes: Uint8Array | null = null;
  let name = '';
  let issuer = '';
  let algorithm: 'SHA1' | 'SHA256' | 'SHA512' = 'SHA1';
  let digits = 6;
  let otpType: 'TOTP' | 'HOTP' = 'TOTP';

  const decoder = new TextDecoder();

  while (reader.hasMore()) {
    const tag = reader.readVarint();
    const fieldNum = tag >> 3;
    const wireType = tag & 0x07;

    if (fieldNum === 1 && wireType === 2) {
      const len = reader.readVarint();
      secretBytes = reader.readBytes(len);
    } else if (fieldNum === 2 && wireType === 2) {
      const len = reader.readVarint();
      name = decoder.decode(reader.readBytes(len));
    } else if (fieldNum === 3 && wireType === 2) {
      const len = reader.readVarint();
      issuer = decoder.decode(reader.readBytes(len));
    } else if (fieldNum === 4 && wireType === 0) {
      const algoVal = reader.readVarint();
      algorithm = algoVal === 2 ? 'SHA256' : algoVal === 3 ? 'SHA512' : 'SHA1';
    } else if (fieldNum === 5 && wireType === 0) {
      const digVal = reader.readVarint();
      digits = digVal === 2 ? 8 : 6;
    } else if (fieldNum === 6 && wireType === 0) {
      const typeVal = reader.readVarint();
      otpType = typeVal === 1 ? 'HOTP' : 'TOTP';
    } else {
      reader.skip(wireType);
    }
  }

  if (!secretBytes || secretBytes.length === 0) return null;

  const secretBase32 = bytesToBase32(secretBytes);

  if (!issuer && name.includes(':')) {
    const parts = name.split(':');
    issuer = parts[0].trim();
    name = parts.slice(1).join(':').trim();
  }

  const id = `${issuer}-${name}-${Math.random().toString(36).substring(2, 7)}`
    .toLowerCase()
    .replace(/\s+/g, '-');

  return {
    id,
    name: name || 'Account',
    issuer: issuer || 'Service',
    secret: secretBase32,
    algorithm,
    digits,
    period: 30,
    otpType,
  };
}
