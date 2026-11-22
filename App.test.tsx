// The Rust root tested from TypeScript: `nsh test` runs this file in a
// headless runner it builds with the crate, and `open(".")` starts the root
// (`nsh.json` "root").
import { expect, open, test } from "nsh/test";

test("shows version and opens add-account and about dialogs", async () => {
  const app = await open(".");
  const initial = app.texts().join("\n");
  expect(initial).toContain("还没有账号");
  expect(initial).toContain("v1.2.0");

  await app.press("role:button[添加账号…]");
  const addTexts = app.texts().join("\n");
  expect(addTexts).toContain("网页授权");
  expect(addTexts).not.toContain("使用企业版（GCP）客户端");
  await app.press("role:tab[手动 Token]");
  expect(app.texts().join("\n")).toContain("导入账号 JSON");
  await app.press("role:tab[本机导入]");
  expect(app.texts().join("\n")).not.toContain("导入账号 JSON");
  await app.press("role:button[关闭]");
  expect(app.texts().join("\n")).not.toContain("网页授权");

  await app.press("role:button[关于]");
  const aboutTexts = app.texts().join("\n");
  expect(aboutTexts).toContain("版本更新");
  expect(aboutTexts).toContain("Apache-2.0");
  await app.press("role:button[关闭]");
  expect(app.texts().join("\n")).not.toContain("版本更新");
});
