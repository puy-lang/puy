program DumpRttiNone;

// A program that asks for no type information has no type information section, and its type
// section and functions are as before (rtti.md 1.6)

type
  TPoint = record
    X, Y: Integer;
  end;
  TShape = class
    P: TPoint;
  end;

var
  S: TShape;
begin
  S := TShape.Create;
  S.P.X := 1;
  WriteLn(S.P.X);
  S.Free;
end.
