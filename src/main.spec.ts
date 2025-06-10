import App from "../App.vue";
import { expect, it } from "vitest";

it("test greeting", async () => {
  expect(App.props.title).toContain("Guess User Age App");
});
