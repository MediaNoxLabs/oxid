// SPDX-License-Identifier: Apache-2.0

// A dependency-free validator for the JSON Schema 2020-12 subset the audit
// contracts use. The repository deliberately avoids adding a schema library:
// `ajv` in the Nix store speaks draft-07, and taking a new runtime dependency
// to check two local files is the wrong trade in a repository that audits its
// own supply chain.
//
// Supported: $ref to #/$defs/*, const, enum, type (incl. integer), minLength,
// maxLength, pattern, minimum, maximum, minItems, items, required, properties,
// additionalProperties: false, propertyNames, oneOf, allOf, and if/then.
//
// Anything outside that set is IGNORED rather than rejected, so a schema that
// grows an unsupported keyword silently loses that constraint. `assertSupported`
// exists to make that failure loud, and the contract tests call it: a schema
// keyword nothing enforces is the same defect class as a check that cannot
// fail.

const SUPPORTED = new Set([
  "$schema", "$id", "$ref", "$defs", "title", "description",
  "const", "enum", "type", "format",
  "minLength", "maxLength", "pattern",
  "minimum", "maximum",
  "minItems", "maxItems", "items",
  "required", "properties", "additionalProperties", "propertyNames",
  "oneOf", "allOf", "if", "then",
]);

function typeOf(value) {
  if (value === null) return "null";
  if (Array.isArray(value)) return "array";
  return typeof value;
}

function matchesType(value, declared) {
  const types = Array.isArray(declared) ? declared : [declared];
  const actual = typeOf(value);
  return types.some((candidate) => {
    if (candidate === "integer") return Number.isInteger(value);
    if (candidate === "number") return actual === "number" && Number.isFinite(value);
    return candidate === actual;
  });
}

function resolveRef(ref, root) {
  if (!ref.startsWith("#/")) throw new Error(`unsupported $ref form: ${ref}`);
  let node = root;
  for (const segment of ref.slice(2).split("/")) {
    const decoded = segment.replace(/~1/gu, "/").replace(/~0/gu, "~");
    const entry = node && typeof node === "object"
      ? Object.entries(node).find(([key]) => key === decoded)
      : undefined;
    node = entry?.[1];
    if (node === undefined) throw new Error(`unresolvable $ref: ${ref}`);
  }
  return node;
}

// Audit schemas are tracked repository contracts, not user-provided regex
// programs. Keep their complete pattern vocabulary explicit so validating an
// untrusted report cannot turn a schema edit into dynamic regular-expression
// execution or main-thread ReDoS.
const SCHEMA_PATTERNS = new Map([
  ["^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$", /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u],
  ["^[0-9a-f]{40}$", /^[0-9a-f]{40}$/u],
  ["^OXA-(MIL|SEC|SUP|ARC|PRC|ANY)-[0-9]{2}$", /^OXA-(MIL|SEC|SUP|ARC|PRC|ANY)-[0-9]{2}$/u],
  ["^[a-z][a-zA-Z0-9]*(\\.[a-zA-Z0-9]+)+$", /^[a-z][a-zA-Z0-9]*(\.[a-zA-Z0-9]+)+$/u],
  ["^[0-9]+(-[0-9]+)?(,[0-9]+(-[0-9]+)?)*$", /^[0-9]+(-[0-9]+)?(,[0-9]+(-[0-9]+)?)*$/u],
  ["^F-[0-9]{2,3}$", /^F-[0-9]{2,3}$/u],
]);

function label(path) {
  return path === "" ? "<root>" : path;
}

function join(path, key) {
  return path === "" ? String(key) : `${path}.${key}`;
}

/**
 * Collect every validation error for `data` against `schema`.
 * Returns an array of `{ path, message }`; empty means valid.
 */
export function validate(schema, data, { root = schema, path = "" } = {}) {
  const errors = [];
  const push = (message, at = path) => errors.push({ path: at, message });

  if (schema.$ref) {
    return validate(resolveRef(schema.$ref, root), data, { root, path });
  }

  if ("const" in schema && data !== schema.const) {
    push(`must equal ${JSON.stringify(schema.const)}`);
  }
  if (schema.enum && !schema.enum.includes(data)) {
    push(`${JSON.stringify(data)} is not one of ${schema.enum.map((v) => JSON.stringify(v)).join(", ")}`);
  }
  if (schema.type && !matchesType(data, schema.type)) {
    const declared = Array.isArray(schema.type) ? schema.type.join("|") : schema.type;
    push(`expected ${declared}, received ${typeOf(data)}`);
    return errors; // further keywords assume the declared type
  }

  if (typeof data === "string") {
    if (schema.minLength !== undefined && data.length < schema.minLength) {
      push(`must be at least ${schema.minLength} characters`);
    }
    if (schema.maxLength !== undefined && data.length > schema.maxLength) {
      push(`must be at most ${schema.maxLength} characters (received ${data.length})`);
    }
    if (schema.pattern) {
      const pattern = SCHEMA_PATTERNS.get(schema.pattern);
      if (!pattern) throw new Error(`unsupported schema pattern: ${schema.pattern}`);
      if (!pattern.test(data)) push(`${JSON.stringify(data)} does not match ${schema.pattern}`);
    }
    if (schema.format === "date-time" && Number.isNaN(Date.parse(data))) {
      push(`${JSON.stringify(data)} is not a date-time`);
    }
    if (schema.format === "uri" && !/^[a-z][a-z0-9+.-]*:/iu.test(data)) {
      push(`${JSON.stringify(data)} is not a URI`);
    }
  }

  if (typeof data === "number") {
    if (schema.minimum !== undefined && data < schema.minimum) push(`must be >= ${schema.minimum}`);
    if (schema.maximum !== undefined && data > schema.maximum) push(`must be <= ${schema.maximum}`);
  }

  if (Array.isArray(data)) {
    if (schema.minItems !== undefined && data.length < schema.minItems) {
      push(`must contain at least ${schema.minItems} item${schema.minItems === 1 ? "" : "s"}`);
    }
    if (schema.maxItems !== undefined && data.length > schema.maxItems) {
      push(`must contain at most ${schema.maxItems} items`);
    }
    if (schema.items) {
      data.forEach((entry, index) => {
        errors.push(...validate(schema.items, entry, { root, path: `${path}[${index}]` }));
      });
    }
  }

  if (data !== null && typeof data === "object" && !Array.isArray(data)) {
    for (const key of schema.required || []) {
      if (!Object.hasOwn(data, key)) push(`missing required property "${key}"`);
    }
    if (schema.additionalProperties === false && schema.properties) {
      for (const key of Object.keys(data)) {
        if (!Object.hasOwn(schema.properties, key)) push(`unexpected property "${key}"`);
      }
    }
    if (schema.propertyNames) {
      for (const key of Object.keys(data)) {
        errors.push(...validate(schema.propertyNames, key, { root, path: join(path, key) }));
      }
    }
    for (const [key, subSchema] of Object.entries(schema.properties || {})) {
      if (Object.hasOwn(data, key)) {
        const value = Object.getOwnPropertyDescriptor(data, key)?.value;
        errors.push(...validate(subSchema, value, { root, path: join(path, key) }));
      }
    }
    if (schema.additionalProperties && typeof schema.additionalProperties === "object") {
      for (const [key, value] of Object.entries(data)) {
        if (!Object.hasOwn(schema.properties || {}, key)) {
          errors.push(...validate(schema.additionalProperties, value, { root, path: join(path, key) }));
        }
      }
    }
  }

  if (schema.oneOf) {
    const matched = schema.oneOf.filter(
      (branch) => validate(branch, data, { root, path }).length === 0,
    );
    if (matched.length !== 1) {
      push(`must match exactly one of ${schema.oneOf.length} alternatives (matched ${matched.length})`);
    }
  }

  for (const subSchema of schema.allOf || []) {
    if (subSchema.if) {
      const applies = validate(subSchema.if, data, { root, path }).length === 0;
      if (applies && subSchema.then) {
        errors.push(...validate(subSchema.then, data, { root, path }));
      }
      continue;
    }
    errors.push(...validate(subSchema, data, { root, path }));
  }

  return errors;
}

/**
 * Walk a schema and report keywords this validator does not enforce.
 *
 * A schema author who adds an unenforced keyword gets silence otherwise, and
 * the constraint they believe they wrote does not exist. The contract tests
 * assert this returns empty for both shipped schemas.
 */
export function assertSupported(schema, path = "") {
  const unsupported = [];
  if (schema === null || typeof schema !== "object") return unsupported;
  if (Array.isArray(schema)) {
    schema.forEach((entry, index) => unsupported.push(...assertSupported(entry, `${path}[${index}]`)));
    return unsupported;
  }
  for (const [key, value] of Object.entries(schema)) {
    if (!SUPPORTED.has(key)) {
      unsupported.push({ path: label(path), keyword: key });
      continue;
    }
    if (key === "properties" || key === "$defs") {
      for (const [name, sub] of Object.entries(value)) {
        unsupported.push(...assertSupported(sub, join(path, `${key}/${name}`)));
      }
      continue;
    }
    if (key === "enum" || key === "required" || key === "type" || key === "const") continue;
    if (typeof value === "object") unsupported.push(...assertSupported(value, join(path, key)));
  }
  return unsupported;
}

/** Format errors for a terminal, most structural first. */
export function formatErrors(errors) {
  return errors.map(({ path, message }) => `  ${label(path)}: ${message}`).join("\n");
}
