program NarrowNotCast;

// Two integer rules where FPC's results differ, so they are fixed here instead of in diff/
// (semantics.md 2.5, 3.2): `not` of a variable of an unsigned type narrower than Integer
// computes in Integer (FPC computes in the width of the type; signed ones agree), and an explicit cast of a variable to a narrower
// type is range checked under {$R+} like an assignment (FPC does not check casts).

uses System.SysUtils;

var
  U8: Byte;
  S8: ShortInt;
  W: Word;
  I: Integer;
begin
  U8 := 255;
  S8 := 0;
  W := 1;
  WriteLn(not U8, ' ', not S8, ' ', not W);
  WriteLn(not U8 and $FF);
  I := 300;
  try
    U8 := Byte(I);
    WriteLn(U8);
  except
    on E: ERangeError do
      WriteLn('ERangeError: ', E.Message);
  end;
{$R-}
  WriteLn(Byte(I));
{$R+}
end.
