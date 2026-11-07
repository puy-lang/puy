program DirIncludeWarn;

// Warnings inside an included file follow the `{$WARN}` state at the including `{$I}` (directives.md chapter 5)
{$I dir_include_warn.inc}
{$WARN ignored-directive OFF}
{$WARN platform-symbol OFF}
{$I dir_include_warn.inc}

begin
end.
