program ClassMethods;

// Methods of classes (objects.md 2.3, 2.4, chapter 5): a virtual call through a variable of the
// parent's type runs the child's override; `inherited Name(…)` and `inherited` alone call the
// parent's method statically, and `inherited` alone does nothing when the parent has no such
// method; members of `Self` are used without `Self.`; overloaded methods are chosen by their
// arguments, also across the parent and the child; `ToString` returns the class name, and
// `ClassName` is the declared name of the instance's class. (Overriding `ToString` is in
// golden/runtime/class_tostring: FPC's `ToString` does not return `string` in delphiunicode mode.)

uses
  System.SysUtils;

type
  TCounter = class
    Count: Integer;
    procedure Add(N: Integer); overload;
    procedure Add(const S: string); overload;
    function Next: Integer;
  end;

  TShape = class
    Name: string;
    function Area: Double; virtual;
    function Describe: string;
    procedure Log(var Lines: string); virtual;
    procedure Reset; virtual;
  end;

  TSquare = class(TShape)
    Side: Double;
    function Area: Double; override;
    procedure Log(var Lines: string); override;
    procedure Reset; override;
  end;

  TCube = class(TSquare)
    function Area: Double; override;
    procedure Log(var Lines: string); override;
  end;

  TCounter2 = class(TCounter)
    procedure Add(A, B: Integer); overload;
  end;

procedure TCounter.Add(N: Integer);
begin
  Count := Count + N;
end;

procedure TCounter.Add(const S: string);
begin
  Add(Length(S));
end;

function TCounter.Next: Integer;
begin
  Add(1);
  Result := Self.Count;
end;

procedure TCounter2.Add(A, B: Integer);
begin
  Add(A);
  Add(B);
end;

function TShape.Area: Double;
begin
  Result := 0;
end;

function TShape.Describe: string;
begin
  Result := Name + ': ' + FloatToStr(Area);
end;

procedure TShape.Log(var Lines: string);
begin
  Lines := Lines + '[shape ' + Name + ']';
end;

procedure TShape.Reset;
begin
  // `TObject` has no `Reset`: does nothing
  inherited;
  Name := 'reset';
end;

function TSquare.Area: Double;
begin
  Result := Side * Side;
end;

procedure TSquare.Log(var Lines: string);
begin
  Lines := Lines + '[square]';
  inherited;
end;

procedure TSquare.Reset;
begin
  inherited Reset;
  Side := 0;
end;

function TCube.Area: Double;
begin
  Result := 6 * inherited Area;
end;

procedure TCube.Log(var Lines: string);
begin
  inherited Log(Lines);
  Lines := Lines + '[cube]';
end;

var
  C: TCounter;
  C2: TCounter2;
  Shapes: array of TShape;
  S: TShape;
  Q: TSquare;
  O: TObject;
  Lines: string;
  I: Integer;
begin
  C := TCounter.Create;
  C.Add(2);
  C.Add('abc');
  WriteLn(C.Count, ' ', C.Next);
  C2 := TCounter2.Create;
  C2.Add(1, 2);
  C2.Add('xy');
  WriteLn(C2.Count);

  SetLength(Shapes, 3);
  Shapes[0] := TShape.Create;
  Q := TSquare.Create;
  Q.Side := 2;
  Shapes[1] := Q;
  Q := TCube.Create;
  Q.Side := 3;
  Shapes[2] := Q;
  for I := 0 to High(Shapes) do
  begin
    S := Shapes[I];
    S.Name := 'n' + IntToStr(I);
    WriteLn(S.Describe);
    Lines := '';
    S.Log(Lines);
    WriteLn(Lines);
    WriteLn(S.ToString, ' ', S.ClassName);
  end;
  S := Shapes[2];
  S.Reset;
  WriteLn(S.Name, ' ', FloatToStr(S.Area));
  O := C;
  WriteLn(O.ToString, ' ', O.ClassName);
  O := Shapes[1];
  WriteLn(O.ToString);

  C.Free;
  C2.Free;
  for I := 0 to High(Shapes) do
    Shapes[I].Free;
end.
