program DumpStmts;

// Shape of statements and expressions in the hir dump (semantics.md chapter 9). Pins only the semantic analysis output

uses System.SysUtils;

var
  G: Integer = 10;
  S: string;

function Square(X: Integer): Integer;
begin
  Square := X * X;
end;

procedure Swap(var A, B: Integer; const Tag: string; out Count: Byte);
var
  T: Integer;
begin
  T := A;
  A := B;
  B := T;
  Count := Length(Tag);
end;

begin
  S := 'ab' + 'c';
  S[1] := Chr(Ord('x') + 1);
  var B: Byte := 1;
  var C := S[2];
  var U: UInt64 := 1;
  Inc(B, 2);
  Dec(G);
  Swap(G, G, S, B);
  if (G > 0) and (B < 3) then
    WriteLn(Square(G):5, ' ', IntToStr(U), Boolean(G), Integer(C), -G, not B)
  else if S = C then
    Halt(1)
  else
    Exit;
  while G > 0 do
    Dec(G);
  repeat
    Inc(G);
    if G = 2 then Break;
  until G >= 3;
  for G := 1 to 3 do
    Write(G);
  for var I := 'a' to 'c' do
    Continue;
  for var J := 3 downto 1 do
    begin end;
  ReadLn(S);
  ReadLn;
  Assert(G <
    5, 'G is ' + IntToStr(G));
  {$C-}
  Assert(False);
  {$C+}
  Write;
  WriteLn(Copy(S, 1, 1), Pos('a', S), Abs(G), ParamCount, ParamStr(0), UpperCase(S), Trim(S), GetEnvironmentVariable('HOME'), StrToIntDef(S, 0) + StrToInt64(S) + StrToInt(S), LowerCase(S), #$D83D#$DE00, #$D800, #$D800'"x');
end.
