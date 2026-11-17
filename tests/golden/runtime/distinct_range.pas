program DistinctRange;

// Under `{$R+}` a value out of the range of a distinct type's root is `ERangeError` named by
// the distinct type (types.md 1.3): an assignment, a value parameter, `Inc`, `Succ` / `Pred`
// and a narrower distinct type of a distinct type. A 64-bit root and a `UInt64` value are
// checked by the conversion to the root, named by the root. The last one is not caught.

uses System.SysUtils;

type
  TSmall = type Byte;
  TUserId = type Int64;
  TTiny = type TSmall;
  TLetter = type Char;

procedure Take(S: TSmall);
begin
  WriteLn(S);
end;

var
  S: TSmall;
  T: TTiny;
  I: Integer;
  L: TLetter;
  Big: Int64;
  U: UInt64;
  Id: TUserId;
begin
  I := 300;
  try
    S := I;
  except
    on E: ERangeError do WriteLn(E.Message);
  end;
  Big := 1000;
  try
    S := Big;
  except
    on E: ERangeError do WriteLn(E.Message);
  end;
  U := 300;
  try
    S := U;
  except
    on E: ERangeError do WriteLn(E.Message);
  end;
  U := High(UInt64);
  try
    Id := U;
  except
    on E: ERangeError do WriteLn(E.Message);
  end;
  try
    Take(I - 50);
  except
    on E: ERangeError do WriteLn(E.Message);
  end;
  S := 255;
  try
    S := Succ(S);
  except
    on E: ERangeError do WriteLn(E.Message);
  end;
  S := 0;
  try
    S := Pred(S);
  except
    on E: ERangeError do WriteLn(E.Message);
  end;
  L := #0;
  try
    L := Pred(L);
  except
    on E: ERangeError do WriteLn(E.Message);
  end;
  I := 255;
  T := I;
  WriteLn(T);
  Inc(T);
end.
