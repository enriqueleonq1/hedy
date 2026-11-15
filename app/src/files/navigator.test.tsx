import { act, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";

import { codeKeys, copyKey, FileNavigatorProvider, sameCopy, useFileNavigator } from "./navigator";

describe("file navigator", () => {
  it("keeps the last request with its view and counts repeats", () => {
    const onOpen = vi.fn();
    const wrapper = ({ children }: { children: ReactNode }) => (
      <FileNavigatorProvider onOpen={onOpen}>{children}</FileNavigatorProvider>
    );
    const { result } = renderHook(() => useFileNavigator(), { wrapper });
    expect(result.current.current).toBeNull();

    const target = { copy: { kind: "task" as const, task: "s5" }, path: "facturas/impuestos.py", line: 16 };
    act(() => result.current.open(target));
    expect(result.current.current).toEqual({ ...target, view: "file" });
    expect(onOpen).toHaveBeenCalledWith({ ...target, view: "file" });
    act(() => result.current.open(target));
    expect(result.current.seq).toBe(2);

    act(() => result.current.open({ ...target, view: "diff", rev: "base" }));
    expect(result.current.current?.view).toBe("diff");
    act(() => result.current.close());
    expect(result.current.current).toBeNull();
  });

  it("is inert outside a provider", () => {
    const { result } = renderHook(() => useFileNavigator());
    act(() => result.current.open({ copy: { kind: "folder", project: "/code/hedy" }, path: "README.md" }));
    expect(result.current.current).toBeNull();
  });

  it("names copies for query keys", () => {
    const folder = { kind: "folder" as const, project: "/code/hedy" };
    expect(copyKey(folder)).toBe("folder:/code/hedy");
    expect(copyKey({ kind: "task", task: "s5" })).toBe("task:s5");
    expect(sameCopy(folder, { kind: "folder", project: "/code/hedy" })).toBe(true);
    expect(codeKeys.file(folder, "a.py").slice(0, 2)).toEqual(codeKeys.copy(folder));
  });
});
