program IntImmediatesChecked;
{$Q+}{$R+}

// Under {$Q+} an operation with a constant operand that fits the 8-bit immediate still raises on
// overflow: Int64 arithmetic keeps its checked instructions (AddIntChk, MulIntChk, DivIntChk), and
// the narrower types put NarrowOvf after the instruction with the immediate (bytecode.md 2.2).
// Not a diff case: FPC widens Integer and Cardinal arithmetic and reports ERangeError instead.

uses System.SysUtils;

var
  L: Int64;
  I: Integer;
  W: Cardinal;
  B: Byte;
  S: SmallInt;

procedure Try1(const Name: string; Op: Integer);
begin
  try
    case Op of
      0: L := L + 1;
      1: L := L - 1;
      2: L := L * 2;
      3: L := L * -1;
      4: L := L div -1;
      5: I := I + 1;
      6: I := I - 127;
      7: I := I * 100;
      8: I := -128 * I;
      9: W := W - 1;
      10: W := W * 2;
      11: B := B + 1;
      12: S := S * 2;
      13: L := L + 127;
    end;
    WriteLn(Name, ' ok');
  except
    on E: EIntOverflow do
      WriteLn(Name, ' EIntOverflow');
    on E: ERangeError do
      WriteLn(Name, ' ERangeError');
  end;
end;

begin
  L := High(Int64); Try1('Int64 + 1', 0); WriteLn(L);
  L := Low(Int64); Try1('Int64 - 1', 1); WriteLn(L);
  L := High(Int64) div 2 + 1; Try1('Int64 * 2', 2); WriteLn(L);
  L := Low(Int64); Try1('Int64 * -1', 3); WriteLn(L);
  L := Low(Int64); Try1('Int64 div -1', 4); WriteLn(L);
  L := High(Int64) - 127; Try1('Int64 + 127', 13); WriteLn(L);
  I := High(Integer); Try1('Integer + 1', 5); WriteLn(I);
  I := Low(Integer) + 126; Try1('Integer - 127', 6); WriteLn(I);
  I := 30000000; Try1('Integer * 100', 7); WriteLn(I);
  I := 20000000; Try1('-128 * Integer', 8); WriteLn(I);
  W := 0; Try1('Cardinal - 1', 9); WriteLn(W);
  W := 3000000000; Try1('Cardinal * 2', 10); WriteLn(W);
  B := 255; Try1('Byte + 1', 11); WriteLn(B);
  S := 20000; Try1('SmallInt * 2', 12); WriteLn(S);
end.
