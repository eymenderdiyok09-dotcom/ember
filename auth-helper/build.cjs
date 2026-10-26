const { buildSync } = require('esbuild');
buildSync({
  entryPoints: [require.resolve('./main.cjs')],
  outfile: require('node:path').join(__dirname, 'dist/main.cjs'),
  bundle: true,
  platform: 'node',
  target: 'node22',
  format: 'cjs',
  minify: true,
  legalComments: 'eof',
});
