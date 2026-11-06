# Lists the fixed English texts of the IDE that `tr()` should translate (and wraps them with `tr()` when run with --wrap).
use strict;
undef $/;
my $wrap = (grep { $_ eq '--wrap' } @ARGV) ? 1 : 0;
my %seen;
my $re = qr/((?:ui\.button|ui\.small_button|ui\.label|ui\.heading|ui\.menu_button|CollapsingHeader::new|Button::new|RichText::new|on_hover_text|hint_text|Window::new|ComboBox::from_label|ui\.link|ui\.collapsing)\(\s*)"((?:[^"\\]|\\.)*)"/;
my $re2 = qr/((?:ui\.checkbox\([^,()]+,|ui\.radio_value\([^,()]+,[^,()]+,|ui\.selectable_value\([^,()]+,[^,()]+,|selectable_label\([^,()]+,)\s*)"((?:[^"\\]|\\.)*)"/;
for my $f (glob("crates/oneiro-ide/src/*.rs")) {
  next if $f =~ /logo|highlight|textops|i18n/;
  open(F, "<$f"); my $s = <F>; close F;
  my $n = 0;
  for my $r ($re, $re2) {
    $s =~ s{$r}{
      my ($pre, $txt) = ($1, $2);
      if (length($txt) > 1 && $txt !~ /\{/ && $txt =~ /[A-Za-z]/) { $seen{$txt}++; $n++; $wrap ? "$pre" . "tr(\"$txt\")" : "$pre\"$txt\"" } else { "$pre\"$txt\"" }
    }ge;
  }
  if ($wrap && $n) { open(F, ">$f"); print F $s; close F; }
}
if ($wrap) { print "wrapped ", scalar(keys %seen), " distinct texts\n"; }
else { print "$_\n" for sort keys %seen; }
