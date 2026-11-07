program DirIgnored;
{$MODE OBJFPC}
{$R foo.res}
{$APPTYPE CONSOLE}
{$TEXTBLOCK CRLF}
{$B+}
{$J-}
{$RTTI EXPLICIT}
{$M+}
{$M 16384,1048576}
{$SCOPEDENUMS ON}
{$HINTS OFF}
{$WARNINGS OFF}
{$O+}
{$HPPEMIT 'x'}
{$PUSH}
{$R-}
{$POP}
{$MODESWITCH ADVANCEDRECORDS}
{$WARN ignored-directive OFF}
{$O-}
{$WARN ignored-directive DEFAULT}
{$D+}
begin
  WriteLn('x');
end.
