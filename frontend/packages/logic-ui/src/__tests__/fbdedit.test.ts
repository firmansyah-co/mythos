// Edits that must not change what a program computes, found in the review of L3c-2 (ADR-013
// §2.8, the Function Block Diagram editor): EN is never numbered as an operand, a pin's edge,
// storage or hidden attribute is never dropped, and deleting a box never drops an assignment.

import { describe, expect, it } from "vitest";
import { remove, setSinkText } from "../fbdedit";
import { open, write, type Root } from "../fbdmodel";
import type { Ids } from "../ldmodel";

const doc = (body: string, vars = "") => `<?xml version="1.0" encoding="utf-8"?>
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types>
    <dataTypes/>
    <pous>
      <pou name="Main" pouType="program">
        <interface><localVars>${vars}</localVars></interface>
        <body>
          <FBD>
${body}
          </FBD>
        </body>
      </pou>
    </pous>
  </types>
</project>
`;
const inv = (id: number, y: number, e: string) =>
  `<inVariable localId="${id}" width="40" height="20"><position x="0" y="${y}"/><connectionPointOut/><expression>${e}</expression></inVariable>`;
const outv = (id: number, y: number, e: string, from: number, formal = "OUT") =>
  `<outVariable localId="${id}" width="40" height="20"><position x="400" y="${y}"/><connectionPointIn><connection refLocalId="${from}" formalParameter="${formal}"/></connectionPointIn><expression>${e}</expression></outVariable>`;
const pin = (f: string, from: number | null, extra = "") =>
  `<variable formalParameter="${f}"${extra}><connectionPointIn>${from === null ? "" : `<connection refLocalId="${from}"/>`}</connectionPointIn></variable>`;
const out = (f: string, extra = "") =>
  `<variable formalParameter="${f}"${extra}><connectionPointOut/></variable>`;
const block = (id: number, type: string, inputs: string, outputs: string) =>
  `<block localId="${id}" typeName="${type}" width="80" height="100"><position x="100" y="0"/><inputVariables>${inputs}</inputVariables><inOutVariables/><outputVariables>${outputs}</outputVariables></block>`;

function opened(text: string) {
  const o = open(text);
  if (!o.ok) throw new Error(o.error);
  return o;
}
const idsOf = (firstFree: number): Ids => ({
  next: firstFree,
  used: new Set(),
});
const inputsOf = (root: Root | null) => {
  if (root?.t !== "box") throw new Error("the root is not a box");
  return root.box.inputs.map(
    (p) => `${p.formal}<-${p.src?.t === "operand" ? p.src.text : "-"}`,
  );
};

describe("the FBD editor keeps what a program computes", () => {
  const enAnd = (inputs: string) =>
    doc(
      [
        inv(1, 10, "en_v"),
        inv(2, 40, "a"),
        inv(3, 70, "b"),
        block(4, "AND", inputs, out("ENO") + out("OUT")),
        outv(5, 10, "q", 4),
      ].join("\n"),
    );

  it("deleting an empty input of an AND with EN numbers the INn inputs again and leaves EN as EN", () => {
    const o = opened(
      enAnd(pin("EN", 1) + pin("IN1", 2) + pin("IN2", 3) + pin("IN3", null)),
    );
    expect(o.model.pous[0]!.networks[0]!.why).toBeNull();
    const r = remove(o.model, {
      kind: "pin",
      pou: 0,
      net: 0,
      box: 4,
      formal: "IN3",
    });
    if (!r.ok) throw new Error(r.error);
    expect(inputsOf(r.model.pous[0]!.networks[0]!.root)).toEqual([
      "EN<-en_v",
      "IN1<-a",
      "IN2<-b",
    ]);
    // As written and read again.
    const again = opened(
      write(o.source, r.model, idsOf(o.source.firstFree)).text,
    );
    expect(inputsOf(again.model.pous[0]!.networks[0]!.root)).toEqual([
      "EN<-en_v",
      "IN1<-a",
      "IN2<-b",
    ]);
  });

  it("an AND with EN and two inputs keeps both inputs", () => {
    const o = opened(enAnd(pin("EN", 1) + pin("IN1", 2) + pin("IN2", null)));
    const r = remove(o.model, {
      kind: "pin",
      pou: 0,
      net: 0,
      box: 4,
      formal: "IN2",
    });
    expect(r.ok).toBe(false);
  });

  it.each([
    ["IN1", ' edge="rising"', ""],
    ["IN1", ' hidden="true"', ""],
    ["OUT", "", ' storage="set"'],
  ])(
    "a network with a pin attribute the editor does not write (%s%s%s) is kept as found",
    (_f, inExtra, outExtra) => {
      const text = doc(
        [
          inv(1, 30, "a"),
          inv(2, 50, "b"),
          block(
            3,
            "AND",
            pin("IN1", 1, inExtra) + pin("IN2", 2),
            out("OUT", outExtra),
          ),
          outv(4, 30, "q", 3),
        ].join("\n"),
      );
      const o = opened(text);
      const net = o.model.pous[0]!.networks[0]!;
      expect(net.why).toMatch(
        /^the pin (IN1|OUT) of block 3 has (edge="rising"|hidden="true"|storage="set"), which this editor does not draw$/,
      );
      // Nothing is put into it, and it is written as it was.
      const ids = idsOf(o.source.firstFree);
      expect(setSinkText(o.model, ids, 0, 0, 4, "q2").ok).toBe(false);
      expect(write(o.source, o.model, ids).text).toBe(text);
    },
  );

  it("deleting a box assigned to two variables is refused, so neither assignment is lost", () => {
    const o = opened(
      doc(
        [
          inv(1, 30, "a"),
          inv(2, 50, "b"),
          block(3, "AND", pin("IN1", 1) + pin("IN2", 2), out("OUT")),
          outv(4, 30, "q", 3),
          outv(5, 70, "r", 3),
        ].join("\n"),
      ),
    );
    const r = remove(o.model, { kind: "element", pou: 0, net: 0, id: 3 });
    expect(r).toEqual({
      ok: false,
      error: "AND is assigned to 2 variables; delete all but one first.",
    });
  });

  it("deleting a box with a second output assigned is refused", () => {
    const o = opened(
      doc(
        [
          inv(1, 30, "go"),
          inv(2, 50, "T#1s"),
          `<block localId="3" typeName="TON" instanceName="t1" width="80" height="100"><position x="100" y="0"/><inputVariables>${pin("IN", 1)}${pin("PT", 2)}</inputVariables><inOutVariables/><outputVariables>${out("Q")}${out("ET")}</outputVariables></block>`,
          outv(4, 30, "lamp", 3, "Q"),
          outv(5, 70, "elapsed", 3, "ET"),
        ].join("\n"),
        '<variable name="t1"><type><derived name="TON"/></type></variable>',
      ),
    );
    expect(o.model.pous[0]!.networks[0]!.why).toBeNull();
    const r = remove(o.model, { kind: "element", pou: 0, net: 0, id: 3 });
    expect(r).toEqual({
      ok: false,
      error: "Delete the assignments of ET first.",
    });
  });

  it("deleting a box assigned to one variable puts its first input in its place", () => {
    const o = opened(
      doc(
        [
          inv(1, 30, "a"),
          inv(2, 50, "b"),
          block(3, "AND", pin("IN1", 1) + pin("IN2", 2), out("OUT")),
          outv(4, 30, "q", 3),
        ].join("\n"),
      ),
    );
    const r = remove(o.model, { kind: "element", pou: 0, net: 0, id: 3 });
    if (!r.ok) throw new Error(r.error);
    const root = r.model.pous[0]!.networks[0]!.root;
    expect(
      root?.t === "sink" &&
        root.sink.text === "q" &&
        root.src?.t === "operand" &&
        root.src.text === "a",
    ).toBe(true);
  });
});
