export default {
  idl: "pinocchio/interface/idl.json",
  scripts: {
    rust: {
      from: "@codama/renderers-rust",
      args: [
        "clients/rust",
        {
          formatCode: true,
          syncCargoToml: false,
          toolchain: "+nightly-2026-01-22",
        },
      ],
    },
  },
};
