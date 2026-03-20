project('events-example', version : '0.1')

weaveback   = find_program('weaveback',   required : true)
asciidoctor = find_program('asciidoctor', required : false)

# Tangle events.adoc into include/events.h, src/events.rs, db/seed_events.sql.
# --stamp writes a sentinel file so ninja can track freshness.
gen = custom_target('gen',
  input   : files('events.adoc'),
  output  : 'gen.stamp',
  command : [
    weaveback, '@INPUT@',
    '--gen', meson.current_source_dir(),
    '--stamp', '@OUTPUT@',
  ],
  build_by_default : true,
)

# Render the literate source to HTML documentation.
if asciidoctor.found()
  custom_target('docs',
    input   : files('events.adoc'),
    output  : 'events.html',
    command : [asciidoctor, '@INPUT@', '-o', '@OUTPUT@'],
    depends : [gen],
    build_by_default : true,
  )
endif
