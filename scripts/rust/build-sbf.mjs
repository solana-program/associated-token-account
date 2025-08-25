#!/usr/bin/env zx
import 'zx/globals';
import {
  cliArguments,
  workingDirectory,
} from '../utils.mjs';

const [folder, ...args] = cliArguments();
const manifestPath = path.join(workingDirectory, folder, 'Cargo.toml');
process.env.CARGO_TARGET_DIR ||= path.join(workingDirectory, 'target', 'sbf');
await $`cargo-build-sbf --manifest-path ${manifestPath} ${args}`;
