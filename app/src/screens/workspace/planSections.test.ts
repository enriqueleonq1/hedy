import { splitAssumptions } from "./planSections";

describe("splitAssumptions", () => {
  it("takes «Lo que supongo» out of the plan", () => {
    const plan = [
      "## Lo que supongo",
      "- A los 15:00 exactos todavía vale",
      "",
      "## Qué voy a cambiar",
      "1. shop/inventory.py",
      "## Cómo lo voy a comprobar",
      "pytest",
    ].join("\n");
    const { assumptions, rest } = splitAssumptions(plan);
    expect(assumptions).toBe("- A los 15:00 exactos todavía vale");
    expect(rest).toBe("## Qué voy a cambiar\n1. shop/inventory.py\n## Cómo lo voy a comprobar\npytest");
  });

  it("finds it in English, bold or further down", () => {
    const { assumptions, rest } = splitAssumptions("Intro\n### **What I assume**\nNothing\n## What I will change\nx");
    expect(assumptions).toBe("Nothing");
    expect(rest).toBe("Intro\n## What I will change\nx");
  });

  it("leaves a plan without the section as it is", () => {
    expect(splitAssumptions("1. Buscar\n2. Cambiar")).toEqual({ assumptions: null, rest: "1. Buscar\n2. Cambiar" });
  });

  it("treats an empty section as none", () => {
    expect(splitAssumptions("## Lo que supongo\n\n## Qué voy a cambiar\nx").assumptions).toBeNull();
  });
});
