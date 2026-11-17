program ForInEnumForms;

uses System.SysUtils;

// The forms of `for..in` with `GetEnumerator` that the diff cases cannot have (objects.md 10.9,
// 13.1, 13.2): `Current` as a function, `for var`, an enumerator returned as an interface (whose
// instance is reference counted or managed by hand), exactly `TObject` with `MoveNext` and
// `Current` from a class helper, `GetEnumerator` from a record helper for `UTF8String`, a
// `for var` variable captured per iteration, and a global loop variable, which gets the last
// element again after the loop even when the body changed it (semantics.md 4.5).

type
  IIntEnum = interface
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TCountedEnum = class(TRefObject, IIntEnum)
    I, N: Integer;
    destructor Destroy; override;
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TManualEnum = class(TObject, IIntEnum)
    I, N: Integer;
    destructor Destroy; override;
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TIntfColl = class
    Manual: Boolean;
    function GetEnumerator: IIntEnum;
  end;

  TCounter = record
    N: Integer;
    function MoveNext: Boolean;
    function Current: Integer;
  end;

  TCountdown = record
    From: Integer;
    function GetEnumerator: TCounter;
  end;

  TObjectEnumHelper = class helper for TObject
    function MoveNext: Boolean;
    function Current: string;
  end;

  TAnyEnum = record
    Counted: Boolean;
    function GetEnumerator: TObject;
  end;

  TUTF8Helper = record helper for UTF8String
    function GetEnumerator: TManualEnum;
  end;

var
  Steps: Integer;
  G: Integer;

destructor TCountedEnum.Destroy;
begin
  WriteLn('TCountedEnum destroyed');
  inherited;
end;

function TCountedEnum.MoveNext: Boolean;
begin
  Inc(I);
  Result := I <= N;
end;

function TCountedEnum.GetCurrent: Integer;
begin
  Result := I;
end;

destructor TManualEnum.Destroy;
begin
  WriteLn('TManualEnum destroyed');
  inherited;
end;

function TManualEnum.MoveNext: Boolean;
begin
  Inc(I);
  Result := I <= N;
end;

function TManualEnum.GetCurrent: Integer;
begin
  Result := -I;
end;

function TIntfColl.GetEnumerator: IIntEnum;
var
  C: TCountedEnum;
  M: TManualEnum;
begin
  if Manual then
  begin
    M := TManualEnum.Create;
    M.N := 2;
    Result := M;
  end
  else
  begin
    C := TCountedEnum.Create;
    C.N := 2;
    Result := C;
  end;
end;

function TCounter.MoveNext: Boolean;
begin
  Dec(N);
  Result := N >= 0;
end;

function TCounter.Current: Integer;
begin
  Result := N;
end;

function TCountdown.GetEnumerator: TCounter;
begin
  Result := Default(TCounter);
  Result.N := From + 1;
end;

function TObjectEnumHelper.MoveNext: Boolean;
begin
  Inc(Steps);
  Result := Steps <= 2;
end;

function TObjectEnumHelper.Current: string;
begin
  Result := ClassName + ' ' + IntToStr(Steps);
end;

function TAnyEnum.GetEnumerator: TObject;
begin
  if Counted then
    Result := TCountedEnum.Create
  else
    Result := TManualEnum.Create;
end;

function TUTF8Helper.GetEnumerator: TManualEnum;
begin
  Result := TManualEnum.Create;
  Result.N := Length(Self);
end;

procedure SetG;
begin
  G := 999;
end;

var
  Coll: TIntfColl;
  Down: TCountdown;
  Any: TAnyEnum;
  U: UTF8String;
  Procs: array of TProc;
begin
  Coll := TIntfColl.Create;
  for var X in Coll do
    WriteLn('counted ', X);
  Coll.Manual := True;
  for var X in Coll do
    WriteLn('manual ', X);
  Coll.Free;

  Down.From := 3;
  for var X in Down do
    Write(X, ' ');
  WriteLn;

  for var S in Any do
    WriteLn(S);
  Steps := 0;
  Any.Counted := True;
  for var S in Any do
    WriteLn(S);

  U := 'héllo';
  for var B in U do
    Write(B, ' ');
  WriteLn;

  for var X in Down do
  begin
    SetLength(Procs, Length(Procs) + 1);
    Procs[High(Procs)] := procedure begin WriteLn('captured ', X) end;
  end;
  for var P in Procs do
    P();

  G := 100;
  Down.From := -1;
  for G in Down do
    WriteLn('never');
  WriteLn('G = ', G);
  Down.From := 2;
  for G in Down do
    SetG;
  WriteLn('G = ', G);
end.
