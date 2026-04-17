// Temporary fixup until Codama macros can express this instruction-level strategy directly
// https://github.com/codama-idl/codama-rs/pull/88

import { readFile, writeFile } from "node:fs/promises";
import { createFromJson, updateInstructionsVisitor } from "codama";

const idlPath = new URL("../pinocchio/interface/idl.json", import.meta.url);
const codama = createFromJson(await readFile(idlPath, "utf8"));

codama.update(
  updateInstructionsVisitor({
    create: { optionalAccountStrategy: "omitted" },
    createIdempotent: { optionalAccountStrategy: "omitted" },
    recoverNested: { optionalAccountStrategy: "omitted" },
  }),
);

await writeFile(
  idlPath,
  `${JSON.stringify(JSON.parse(codama.getJson()), null, 2)}\n`,
);
