program AssertValuesDisplay;

uses System.SysUtils;

// Each failed Assert records its subexpressions with the display rules of every kind of value
// (docs/spec/semantics.md 5.4); the values are read back through EAssertionFailed.Values.

type
  TColor = (Red, Green, Blue);
  TDigit = 0..9;
  TPoint = record
    X, Y: Integer;
  end;
  TBox = record
    Corner: TPoint;
    Name: string;
  end;
  TLevel3 = record
    Inner: TBox;
  end;
  TLevel4 = record
    Deep: TLevel3;
  end;
  TPair = array[1..2] of TPoint;
  TWide = set of Char;
  IShape = interface
    function Area: Integer;
  end;
  TShape = class(TRefObject, IShape)
  private
    FSecret: Integer;
  public
    function Area: Integer;
  end;
  TShapeClass = class of TShape;
  TNotify = procedure;

function TShape.Area: Integer;
begin
  Result := FSecret;
end;

procedure Show(const E: EAssertionFailed);
begin
  WriteLn(E.Message);
  for var V in E.Values do
    WriteLn('  ', V.Expr, ' = ', V.Value);
end;

procedure Nothing;
begin
end;

function Same(const B: TBox; const L: TLevel4; const P: TPair): Boolean;
begin
  Result := (B.Name = '') and (L.Deep.Inner.Name = 'x') and (P[1].X = 5);
end;

var
  I: Integer;
  U: UInt64;
  B: Boolean;
  C: Char;
  S: string;
  U8: UTF8String;
  Sg: Single;
  D: Double;
  Cur: Currency;
  Color: TColor;
  Digit: TDigit;
  Colors: set of TColor;
  Wide: TWide;
  P: TPoint;
  Box: TBox;
  L4: TLevel4;
  Pair: TPair;
  Arr: TArray<Integer>;
  Empty: TArray<string>;
  Shape: TShape;
  NoShape: TShape;
  Intf: IShape;
  Cls: TShapeClass;
  Proc: TNotify;
  NoProc: TNotify;
begin
  I := -7;
  U := High(UInt64);
  B := True;
  C := #9;
  S := 'it''s'#10'ok';
  U8 := 'héllo';
  Sg := 0.1;
  D := 1e100;
  Cur := -0.5;
  Color := TColor.Green;
  Digit := 3;
  Colors := [TColor.Red, TColor.Blue];
  Wide := ['a', 'z'];
  P.X := 1;
  P.Y := 2;
  Box.Corner := P;
  Box.Name := 'box';
  L4 := Default(TLevel4);
  Pair := Default(TPair);
  Pair[2] := P;
  Arr := [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
  Empty := nil;
  Shape := TShape.Create;
  Shape.FSecret := 42;
  NoShape := nil;
  Intf := Shape;
  Cls := TShape;
  Proc := Nothing;
  NoProc := nil;
  try
    Assert((I = 0) or (U = 0) or not B or (C = 'x') or (S = '') or (U8 = ''));
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    Assert((Sg > 1) or (D < 0) or (Cur > 0) or (Color = TColor.Red) or (Digit = 0));
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    Assert((Colors = []) or (Wide = []) or (P.X = 0) or (Box.Name = ''));
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    Assert((L4.Deep.Inner.Corner.X = 1) or (Pair[1].X = 5) or (Length(Arr) = 0) or (Length(Empty) = 1));
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    Assert(Same(Box, L4, Pair));
  except
    on E: EAssertionFailed do Show(E);
  end;
  try
    Assert((Shape = nil) or (NoShape <> nil) or (Intf = nil) or (Cls = nil) or not Assigned(Proc) or Assigned(NoProc));
  except
    on E: EAssertionFailed do Show(E);
  end;
  // A long value is cut to 100 characters
  S := StringOfChar('x', 150);
  try
    Assert(S = '');
  except
    on E: EAssertionFailed do Show(E);
  end;
end.
