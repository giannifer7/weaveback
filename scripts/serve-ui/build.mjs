import * as esbuild from 'esbuild';
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const dir = dirname(fileURLToPath(import.meta.url));

const result = await esbuild.build({
  entryPoints: [`${dir}/src/index.ts`],
  bundle: true,
  minify: true,
  write: false,
  format: 'iife',
  target: ['chrome120', 'firefox121', 'safari17'],
  treeShaking: true,
});

const js = new TextDecoder().decode(result.outputFiles[0].contents);
const out = join(dir, '../asciidoc-theme/docinfo-footer.html');
writeFileSync(out, `<script>\n${js}\n</script>\n`);
console.log('serve-ui: built \u2192 docinfo-footer.html');
