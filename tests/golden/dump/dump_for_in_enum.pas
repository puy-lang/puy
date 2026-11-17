program DumpForInEnum;

// The dumps of `for..in` with `GetEnumerator` (objects.md 13.2, 14.10, 15.6): `for-in-enum` with
// the hidden variables of the collection and the enumerator, and the calls of `GetEnumerator`,
// `MoveNext` and `Current`; the loop is the body of a `try` whose `finally` block is `Free` for an
// enumerator managed by hand, `Release` for a reference counted one and a check of the contents
// (`IsClass` with `TRefObject`) for an interface. A global loop variable has a hidden variable
// for the element it gets back after the loop. A captured `for var` variable gets one new cell
// per iteration.

type
  IEnum = interface
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TEnum = class
    function MoveNext: Boolean;
    function Current: Integer;
  end;

  TCountedEnum = class(TRefObject)
    function MoveNext: Boolean;
    function Current: Integer;
  end;

  TColl = class
    function GetEnumerator: TEnum;
  end;

  TCountedColl = class(TRefObject)
    function GetEnumerator: TCountedEnum;
  end;

  TIntfColl = class
    function GetEnumerator: IEnum;
  end;

function TEnum.MoveNext: Boolean;
begin
  Result := False;
end;

function TEnum.Current: Integer;
begin
  Result := 0;
end;

function TCountedEnum.MoveNext: Boolean;
begin
  Result := False;
end;

function TCountedEnum.Current: Integer;
begin
  Result := 0;
end;

function TColl.GetEnumerator: TEnum;
begin
  Result := TEnum.Create;
end;

function TCountedColl.GetEnumerator: TCountedEnum;
begin
  Result := TCountedEnum.Create;
end;

function TIntfColl.GetEnumerator: IEnum;
begin
  Result := nil;
end;

procedure Manual(C: TColl);
begin
  for var X in C do
    WriteLn(X);
end;

procedure Counted(C: TCountedColl);
begin
  for var X in C do
    WriteLn(X);
end;

procedure ByContents(C: TIntfColl);
begin
  for var X in C do
    WriteLn(X);
end;

procedure Captured(C: TCountedColl);
var
  P: TProc;
begin
  for var X in C do
    P := procedure begin WriteLn(X) end;
end;

var
  G: Integer;
  C: TColl;
begin
  C := TColl.Create;
  for G in C do
    WriteLn(G);
  Manual(C);
  Counted(TCountedColl.Create);
  Captured(TCountedColl.Create);
  C.Free;
end.
