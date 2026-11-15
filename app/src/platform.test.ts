import { isMac, keyLabels } from "./platform";

describe("platform", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("shows ⌘ on macOS and Ctrl elsewhere", () => {
    expect(keyLabels(true)).toMatchObject({ enter: "⏎", newline: "⇧⏎", submit: "⌘⏎" });
    expect(keyLabels(true).jump(3)).toBe("⌘3");
    expect(keyLabels(false)).toMatchObject({ enter: "Enter", newline: "Shift+Enter", submit: "Ctrl+Enter" });
    expect(keyLabels(false).jump(3)).toBe("Ctrl+3");
  });

  it("detects macOS from the navigator", () => {
    vi.stubGlobal("navigator", { platform: "MacIntel", userAgent: "" });
    expect(isMac()).toBe(true);
    vi.stubGlobal("navigator", { userAgentData: { platform: "Linux" }, platform: "", userAgent: "" });
    expect(isMac()).toBe(false);
    vi.stubGlobal("navigator", { platform: "Win32", userAgent: "Mozilla/5.0 (Windows NT 10.0)" });
    expect(isMac()).toBe(false);
  });
});
