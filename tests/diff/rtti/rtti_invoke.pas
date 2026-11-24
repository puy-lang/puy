program RttiInvoke;

// TRttiMethod.Invoke and the properties read and written by their methods (rtti.md 5.6, 5.7):
// a virtual method called through the entry of the class declaring it runs the override of the
// instance's class, a class method runs the one of the class reference's class (given a class
// reference; an instance is EInvalidCast, see golden/runtime/rtti_invoke_puy), a constructor
// invoked with a class reference creates an instance of that class, the var and out arguments
// are written back into the arguments' array, and an exception of the method reaches the caller
// as it was raised.

uses System.SysUtils, System.TypInfo, System.Rtti;

type
  TShape = class
  private
    FSize: Integer;
    function GetArea: Integer;
    procedure SetSize(V: Integer);
  public
    Name: string;
    constructor Create(const AName: string);
    function Describe(Times: Integer): string; virtual;
    procedure Grow(var By: Integer; out Before: Integer);
    procedure Fail(const Msg: string);
    class function Kind: string; virtual;
    class function Twice(X: Integer): Integer; static;
    property Area: Integer read GetArea;
    property Size: Integer read FSize write SetSize;
  end;

  TCircle = class(TShape)
  public
    function Describe(Times: Integer): string; override;
    class function Kind: string; override;
  end;

constructor TShape.Create(const AName: string);
begin
  Name := AName;
  FSize := 2;
end;

function TShape.GetArea: Integer;
begin
  Result := FSize * FSize;
end;

procedure TShape.SetSize(V: Integer);
begin
  FSize := V * 10;
end;

function TShape.Describe(Times: Integer): string;
begin
  Result := 'shape ' + Name + ' x' + IntToStr(Times);
end;

procedure TShape.Grow(var By: Integer; out Before: Integer);
begin
  Before := FSize;
  FSize := FSize + By;
  By := By * 2;
end;

procedure TShape.Fail(const Msg: string);
begin
  raise EConvertError.Create(Msg);
end;

class function TShape.Kind: string;
begin
  Result := 'shape kind';
end;

class function TShape.Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

function TCircle.Describe(Times: Integer): string;
begin
  Result := 'circle ' + Name + ' x' + IntToStr(Times);
end;

class function TCircle.Kind: string;
begin
  Result := 'circle kind';
end;

var
  Ctx: TRttiContext;
  T: TRttiType;
  S: TShape;
  V: TValue;
  A: TArray<TValue>;
begin
  Ctx := TRttiContext.Create;
  T := Ctx.GetType(TShape);
  V := T.GetMethod('Create').Invoke(TCircle, ['c1']);
  S := V.AsType<TShape>;
  WriteLn(S.ClassName, ' ', S.Name);
  WriteLn(T.GetMethod('Describe').Invoke(S, [3]).AsString);
  WriteLn(T.GetMethod('Kind').Invoke(TCircle, []).AsString);
  WriteLn(T.GetMethod('Twice').Invoke(TShape, [21]).AsInteger);
  SetLength(A, 2);
  A[0] := 5;
  A[1] := 0;
  WriteLn(T.GetMethod('Grow').Invoke(S, A).IsEmpty);
  WriteLn(A[0].AsInteger, ' ', A[1].AsInteger, ' ', T.GetProperty('Size').GetValue(S).AsInteger);
  WriteLn(T.GetProperty('Area').GetValue(S).AsInteger);
  T.GetProperty('Size').SetValue(S, 3);
  WriteLn(T.GetProperty('Size').GetValue(S).AsInteger);
  try
    T.GetMethod('Fail').Invoke(S, ['bad']);
  except
    on E: Exception do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  S.Free;
end.
