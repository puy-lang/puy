program RttiMembers;

// System.Rtti over classes with public members only (Delphi lists private fields too): the
// order of GetFields / GetProperties (the type's own, then its ancestors'), the visibility and
// method kinds, the parameter flags, reading and writing fields and properties, and the range of
// an enumeration (rtti.md 5.1, 5.4 to 5.6)

uses System.SysUtils, System.TypInfo, System.Rtti;

type
  TColor = (Red, Green, Blue);

  TShape = class
  public
    Name: string;
    Color: TColor;
    Width: Integer;
    property W: Integer read Width write Width;
    property ShapeName: string read Name;
    procedure Draw(Times: Integer; var Done: Boolean; out Msg: string; const Tag: Integer);
    function Area: Double;
    class function Count: Integer;
  end;

  TCircle = class(TShape)
  public
    Radius: Double;
    Center: Integer;
    property R: Double read Radius write Radius;
  end;

procedure TShape.Draw(Times: Integer; var Done: Boolean; out Msg: string; const Tag: Integer);
begin
  Msg := '';
end;

function TShape.Area: Double;
begin
  Result := 0;
end;

class function TShape.Count: Integer;
begin
  Result := 0;
end;

procedure ShowFlags(Flags: TParamFlags);
var
  F: TParamFlag;
begin
  for F := Low(TParamFlag) to High(TParamFlag) do
    if F in Flags then
      Write(' ', Ord(F));
  WriteLn;
end;

var
  Ctx: TRttiContext;
  T: TRttiType;
  F: TRttiField;
  P: TRttiProperty;
  M: TRttiMethod;
  A: TRttiParameter;
  C: TCircle;
begin
  WriteLn(Ord(TMemberVisibility.mvPublic), ' ', Ord(TMemberVisibility.mvPublished));
  WriteLn(Ord(TMethodKind.mkConstructor), ' ', Ord(TMethodKind.mkClassFunction), ' ',
    Ord(TMethodKind.mkSafeFunction));
  WriteLn(Ord(TParamFlag.pfVar), ' ', Ord(TParamFlag.pfConst), ' ', Ord(TParamFlag.pfArray), ' ',
    Ord(TParamFlag.pfOut), ' ', Ord(TParamFlag.pfResult));
  WriteLn(Ord(TOrdType.otULong), ' ', Ord(TFloatType.ftDouble), ' ', Ord(TFloatType.ftCurr));

  Ctx := TRttiContext.Create;
  T := Ctx.GetType(TCircle);
  WriteLn(T.Name, ' ', T.BaseType.Name, ' ', T.IsInstance);
  for F in T.GetFields do
    WriteLn('field ', F.Name, ': ', F.FieldType.Name, ' of ', F.Parent.Name, ' ',
      Ord(F.Visibility));
  for F in T.GetDeclaredFields do
    WriteLn('declared field ', F.Name);
  for P in T.GetProperties do
    WriteLn('property ', P.Name, ': ', P.PropertyType.Name, ' ', P.IsReadable, ' ',
      P.IsWritable);

  for M in Ctx.GetType(TShape).GetDeclaredMethods do
  begin
    Write('method ', M.Name, ' ', Ord(M.MethodKind), ' ', M.IsClassMethod);
    if M.ReturnType <> nil then
      Write(' -> ', M.ReturnType.Name);
    WriteLn;
    for A in M.GetParameters do
    begin
      Write('  ', A.Name, ': ', A.ParamType.Name);
      ShowFlags(A.Flags);
    end;
  end;

  C := TCircle.Create;
  try
    C.Name := 'disc';
    T.GetField('Radius').SetValue(C, TValue.From<Double>(2.5));
    T.GetField('Width').SetValue(C, 7);
    T.GetField('Color').SetValue(C, TValue.From<TColor>(TColor.Blue));
    T.GetProperty('W').SetValue(C, 9);
    WriteLn(C.Width, ' ', C.Radius:0:1, ' ', Ord(C.Color));
    WriteLn(T.GetField('Name').GetValue(C).AsString, ' ', T.GetField('Color').GetValue(C).ToString,
      ' ', T.GetProperty('R').GetValue(C).AsExtended:0:1, ' ',
      T.GetProperty('ShapeName').GetValue(C).ToString);
  finally
    C.Free;
  end;

  WriteLn((Ctx.GetType(TypeInfo(TColor)) as TRttiEnumerationType).MaxValue);
  Ctx.Free;
end.
