program DirCommaPartial;

// Comma form mixing switches that take effect and switches that are ignored (directives.md chapter 1)
{$R-}
{$R+}
{$R+}
// Removing a part that changes meaning in other compilers is only a suggestion (directives.md 2.3)
{$R+,J-}
{$X-,B-}
{$IFOPT R+}{$MESSAGE HINT 'R on'}{$ENDIF}

begin
end.
