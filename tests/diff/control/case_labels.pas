program CaseLabels;

// `case` labels (semantics.md 4.10) written with constants and constant expressions, `Ord`
// and `Chr` of constants, negative and Int64 ranges, `Word` and `ShortInt` selectors, an
// expression selector, `Exit` and `Break` inside arms, and a `case` in a function result.

const
  Lo = 10;
  Hi = Lo * 2;
  Mid = (Lo + Hi) div 2;
  Big = 3000000000;

type
  TOp = (Add, Sub, Mul, Divide);

function Apply(Op: TOp; A, B: Integer): Integer;
begin
  case Op of
    TOp.Add: Result := A + B;
    TOp.Sub: Result := A - B;
    TOp.Mul: Result := A * B;
  else
    begin
      if B = 0 then
      begin
        Result := 0;
        Exit;
      end;
      Result := A div B;
    end;
  end;
end;

function Band(N: Integer): string;
begin
  case N of
    Low(Integer)..Lo - 1: Result := 'below';
    Lo..Mid - 1: Result := 'lower half';
    Mid: Result := 'middle';
    Mid + 1..Hi: Result := 'upper half';
  else
    Result := 'above';
  end;
end;

var
  I: Integer;
  W: Word;
  S: ShortInt;
  L: Int64;
  Op: TOp;
begin
  for I := 8 to 22 do
    Write(Band(I), '; ');
  WriteLn;
  for Op := Low(TOp) to High(TOp) do
    Write(Apply(Op, 12, 4), ' ');
  WriteLn(Apply(TOp.Divide, 1, 0));

  W := 65535;
  case W of
    0..255: WriteLn('byte');
    256..65534: WriteLn('word');
    65535: WriteLn('max word');
  end;
  S := -128;
  case S of
    -128..-1: WriteLn('negative');
    0..127: WriteLn('non-negative');
  end;
  L := Big;
  case L of
    Low(Int64)..High(Integer): WriteLn('fits in Integer');
    Big: WriteLn('big');
  end;
  case Ord('b') of
    Ord('a'): WriteLn('a');
    Ord('b')..Ord('z'): WriteLn('b..z');
  end;
  case Chr(Ord('A') + 2) of
    Chr(65)..'B': WriteLn('A or B');
    'C': WriteLn('C');
  end;

  // An expression selector, and `Break` / `Continue` from an arm inside a loop
  for I := 1 to 10 do
  begin
    case I mod 4 of
      0: Continue;
      3:
        if I > 6 then
          Break;
    end;
    Write(I, ' ');
  end;
  WriteLn;
end.
