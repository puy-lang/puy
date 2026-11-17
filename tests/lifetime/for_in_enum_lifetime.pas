program ForInEnumeratorsLifetime;

uses System.SysUtils;

// When `for..in` lets go of its enumerator (objects.md 13.2): a reference counted enumerator is
// destroyed when the loop ends unless something else holds it; one managed by hand is destroyed
// even when `GetEnumerator` returns an instance the collection keeps; a record enumerator's
// managed fields are let go of; the collection's reference held by the loop lives to the end of
// the routine. A destructor raising in the cleanup puts the body's exception in its `Suppressed`
// (8.5).

type
  TLog = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TCountedEnum = class(TRefObject)
    I: Integer;
    destructor Destroy; override;
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TColl = class(TRefObject)
    Keep: TCountedEnum;
    destructor Destroy; override;
    function GetEnumerator: TCountedEnum;
  end;

  TManualEnum = class
    I: Integer;
    destructor Destroy; override;
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TKeeper = class
    Enum: TManualEnum;
    function GetEnumerator: TManualEnum;
  end;

  TRecEnum = record
    Log: TLog;
    I: Integer;
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TRecColl = record
    function GetEnumerator: TRecEnum;
  end;

  TBadEnum = class
    I: Integer;
    destructor Destroy; override;
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TBadColl = class
    function GetEnumerator: TBadEnum;
  end;

constructor TLog.Create(const AName: string);
begin
  Name := AName;
end;

destructor TLog.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

destructor TCountedEnum.Destroy;
begin
  WriteLn('destroy counted enumerator');
  inherited;
end;

function TCountedEnum.MoveNext: Boolean;
begin
  Inc(I);
  Result := I <= 2;
end;

function TCountedEnum.GetCurrent: Integer;
begin
  Result := I;
end;

destructor TColl.Destroy;
begin
  WriteLn('destroy collection');
  inherited;
end;

function TColl.GetEnumerator: TCountedEnum;
begin
  if Keep = nil then
    Exit(TCountedEnum.Create);
  Keep.I := 0;
  Result := Keep;
end;

destructor TManualEnum.Destroy;
begin
  WriteLn('destroy manual enumerator');
  inherited;
end;

function TManualEnum.MoveNext: Boolean;
begin
  Inc(I);
  Result := I <= 1;
end;

function TManualEnum.GetCurrent: Integer;
begin
  Result := I;
end;

function TKeeper.GetEnumerator: TManualEnum;
begin
  Result := Enum;
end;

function TRecEnum.MoveNext: Boolean;
begin
  Inc(I);
  Result := I <= 1;
end;

function TRecEnum.GetCurrent: Integer;
begin
  Result := I;
end;

function TRecColl.GetEnumerator: TRecEnum;
begin
  Result := Default(TRecEnum);
  Result.Log := TLog.Create('record enumerator field');
end;

destructor TBadEnum.Destroy;
begin
  WriteLn('destroy bad enumerator');
  raise Exception.Create('from the destructor');
end;

function TBadEnum.MoveNext: Boolean;
begin
  Inc(I);
  Result := True;
end;

function TBadEnum.GetCurrent: Integer;
begin
  Result := I;
end;

function TBadColl.GetEnumerator: TBadEnum;
begin
  Result := TBadEnum.Create;
end;

function MakeColl: TColl;
begin
  Result := TColl.Create;
end;

procedure Counted;
begin
  for var X in MakeColl do
    WriteLn('counted ', X);
  WriteLn('after the loop');
end;

procedure Kept;
var
  C: TColl;
begin
  C := TColl.Create;
  C.Keep := TCountedEnum.Create;
  for var X in C do
    WriteLn('kept ', X);
  WriteLn('after the loop, kept ', C.Keep.I);
end;

procedure Manual;
var
  K: TKeeper;
begin
  K := TKeeper.Create;
  K.Enum := TManualEnum.Create;
  for var X in K do
    WriteLn('manual ', X);
  WriteLn('after the loop, assigned ', Assigned(K.Enum));
  K.Free;
end;

procedure Rec;
var
  R: TRecColl = Default(TRecColl);
begin
  for var X in R do
    WriteLn('record ', X);
  WriteLn('after the loop');
end;

procedure Bad;
var
  B: TBadColl;
begin
  B := TBadColl.Create;
  try
    for var X in B do
      if X = 2 then
        raise Exception.Create('from the body');
  finally
    B.Free;
  end;
end;

begin
  Counted;
  WriteLn('---');
  Kept;
  WriteLn('---');
  Manual;
  WriteLn('---');
  Rec;
  WriteLn('---');
  try
    Bad;
  except
    on E: Exception do
      WriteLn(E.Message, ', suppressed: ', E.Suppressed[0].Message);
  end;
end.
