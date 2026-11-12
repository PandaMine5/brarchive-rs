// deno.json cannot inherit the Cargo workspace version, so keep the two in
// sync by hand and let this test catch drift.
import { assertEquals } from "@std/assert";
import { parse } from "@std/toml";

Deno.test("deno.json version matches the Cargo workspace version", async () => {
  const denoJson = JSON.parse(
    await Deno.readTextFile(new URL("./deno.json", import.meta.url)),
  );
  const cargoToml = parse(
    await Deno.readTextFile(new URL("../../Cargo.toml", import.meta.url)),
  ) as { workspace: { package: { version: string } } };

  assertEquals(
    denoJson.version,
    cargoToml.workspace.package.version,
    "bump crates/wasm/deno.json together with [workspace.package] version",
  );
});
