program DirCommaPartial;

// Comma form mixing switches that take effect and switches that are ignored (directives.md chapter 1)
{$R-}
{$R+,X-}
{$X-,R+}
{$X-,Y+}
{$IFOPT R+}{$MESSAGE HINT 'R on'}{$ENDIF}

begin
end.
