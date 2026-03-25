project('nim-adoc-example', version : '0.1.0', meson_version : '>=1.1.0')

python3      = find_program('python3',      required : true)
weaveback    = find_program('weaveback',    required : true)
nph          = find_program('nph',          required : false)
asciidoctor  = find_program('asciidoctor',  required : false)

# ── Weaveback flags for AsciiDoc literate sources ─────────────────────────────
#
# The <[ ]> delimiters and '#' comment marker are the convention used in this
# project.  Adjust --chunk-end if your sources use a different end marker.
weaveback_flags = [
  '--open-delim',      '<[',
  '--close-delim',     ']>',
  '--chunk-end',       '@',
  '--comment-markers', '#',
]
if nph.found()
  weaveback_flags += ['--formatter', 'nim=' + nph.full_path()]
endif

# ── Code generation ───────────────────────────────────────────────────────────
#
# Discovers all .adoc driver files under the source directory, writes generated
# files into gen/, and produces a depfile so ninja reruns on any change.
gen_nim = custom_target('gen-nim',
  output  : ['gen_nim.stamp'],
  depfile : 'gen_nim.d',
  command : [
    weaveback,
    '--dir',     meson.current_source_dir(),
    '--ext',     'adoc',
    '--include', meson.current_source_dir(),
    '--gen',     meson.current_source_dir() / 'gen',
    '--db',      meson.current_source_dir() / 'weaveback.db',
    '--stamp',   '@OUTPUT0@',
    '--depfile', '@DEPFILE@',
  ] + weaveback_flags,
  build_by_default : true,
)

# ── HTML documentation ────────────────────────────────────────────────────────
#
# Renders the literate source to HTML using Asciidoctor with the Gruvbox Dark
# theme (theme/docinfo.html injected via docinfo=shared).
# Output goes to docs/html/ in the source tree so it can be browsed directly.
if asciidoctor.found()
  custom_target('gen-docs',
    input   : files('config.nim.adoc'),
    output  : ['gen_docs.stamp'],
    command : [
      python3, meson.current_source_dir() / 'scripts' / 'gen_docs.py',
      asciidoctor,
      '-r', 'asciidoctor-diagram',
      '-a', 'source-highlighter=rouge',
      '-a', 'rouge-css=class',
      '-a', 'rouge-style=gruvbox',
      '-a', 'docinfo=shared',
      '-a', 'docinfodir=' + meson.current_source_dir() / 'theme',
      '-a', 'imagesdir=.',
      '-D', meson.current_source_dir() / 'docs' / 'html',
      '@INPUT@',
      '@OUTPUT0@',
    ],
    build_by_default : true,
    depends : [gen_nim],
  )
endif
