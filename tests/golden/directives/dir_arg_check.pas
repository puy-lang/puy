program DirArgCheck;

// Argument checks for argumentless `{$R}` and for `{$INLINE}` / `{$LEGACYIFEND}` (directives.md 2.2, 2.3);
// the remaining count of W0007 counts only evaluated uses (3.3)
{$R}
{$INLINE MAYBE}
{$INLINE AUTO}
{$LEGACYIFEND YES}
{$LEGACYIFEND OFF}
{$IFDEF VER350}{$ENDIF}
{$IFDEF NOPE}
{$IFDEF VER340}{$ENDIF}
{$ENDIF}
{$IFDEF DCC}{$ENDIF}

begin
end.
