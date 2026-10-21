# Stub for YAML::Syck, which txt2html 3.0 `use`s at load time (lib/HTML/
# TextToHTML.pm:638) but never calls -- there is no LoadFile/DumpFile anywhere
# in the module or in scripts/txt2html.  Installing the real YAML::Syck is not
# possible here (it is a YAML 1.0 emitter and has been superseded by
# YAML::XS/YAML::PP on every current perl), and nothing that runs touches it.
#
# This is the canonical copy. `make ref` copies it to ref/stubs/YAML/Syck.pm,
# which is the path the differential harness puts on PERL5LIB.  The copy under
# ref/ is derived and gitignored; this file is the one that is versioned, so
# the reasoning above travels with the code and can be reviewed.
package YAML::Syck;
our $VERSION = '1.00';
1;
