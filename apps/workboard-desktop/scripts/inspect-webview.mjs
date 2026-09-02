import { chromium } from "playwright";

const [, , action = "read", argument] = process.argv;
const browser = await chromium.connectOverCDP("http://127.0.0.1:9222");
const page = browser.contexts()[0].pages()[0];
page.on("console", (message) => console.log(`[${message.type()}] ${message.text()}`));
page.on("pageerror", (error) => console.log(`[pageerror] ${error.message}`));

const dump = async (label) => {
  console.log(`\n===== ${label} =====`);
  console.log(await page.evaluate(() => location.pathname + location.search));
  console.log(await page.evaluate(() => document.body.innerText.slice(0, 2500)));
};

if (action === "read") {
  await page.waitForTimeout(3000);
  await dump("initial");
} else if (action === "click") {
  await page.getByRole("link", { name: argument, exact: true }).first().click();
  await page.waitForTimeout(1500);
  await dump(`after clicking ${argument}`);
} else if (action === "expand") {
  await page.getByRole("button", { name: argument }).first().click();
  await page.waitForTimeout(800);
  await dump(`after ${argument}`);
} else if (action === "goto") {
  await page.evaluate((path) => history.pushState(null, "", path), argument);
  await page.evaluate(() => window.dispatchEvent(new PopStateEvent("popstate")));
  await page.waitForTimeout(1500);
  await dump(`after navigating to ${argument}`);
} else if (action === "sidebar") {
  await page.waitForTimeout(1500);
  console.log(await page.getByRole("complementary", { name: "Workspace navigation" }).innerText());
}

await browser.close();
