program ConstConversions;

// An E0714 fix whose hex form equals the original is given in decimal; an `Abs` constant not fitting `Int64` is E0714;
// the type name of `Char(constant)` is `Char`; no `IntToStr(...)` fix when a user `IntToStr` exists (2.5, chapter 3, 5.1)
var
  Sh: ShortInt;
  I: Integer;
  C: Char;
  XX: Integer;

function IntToStr(X: Integer): string;
begin
  Result := 'n';
end;

const
  AH = Abs(-9223372036854775808);

begin
  Sh := ShortInt($80);
  I := Integer($FFFFFFFF);
  I := Integer($1FFFFFFFF);
  C := Char(70000);
  C := Chr(-1);
  XX := 1;
  WriteLn('Total: ' + XX);
  Inc(I, 'a');
  Inc(C, True);
  WriteLn(Sh, I, C, AH);
end.
