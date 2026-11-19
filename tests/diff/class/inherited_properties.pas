program InheritedProperties;

// `inherited P` (objects.md 2.4): a property the child hides with one of the same name is read
// and written through the parent's declaration: a field accessor, accessor methods, an indexed
// property, a default array property named with `inherited Items[…]`, `index N` and a
// `class property` (also from a class method). Also used from a nested routine and in an
// expression with `Self`'s own `P`.

type
  TBase = class
  private
    FCount: Integer;
    FNames: array[0..2] of string;
    function GetName(I: Integer): string;
    procedure SetName(I: Integer; const Value: string);
    function GetValue(const Key: string): string;
    procedure SetValue(const Key: string; const Value: string);
    function GetPart(Index: Integer): Integer;
    class function GetKind: string; static;
  public
    property Count: Integer read FCount write FCount;
    property Names[I: Integer]: string read GetName write SetName;
    property Values[const Key: string]: string read GetValue write SetValue; default;
    property Low: Integer index 1 read GetPart;
    class property Kind: string read GetKind;
  end;

  TChild = class(TBase)
  private
    function GetCount: Integer;
    procedure SetCount(Value: Integer);
    function GetName2(I: Integer): string;
    function GetValue2(const Key: string): string;
    procedure SetValue2(const Key: string; const Value: string);
    function GetLow: Integer;
    class function GetKind2: string; static;
  public
    property Count: Integer read GetCount write SetCount;
    property Names[I: Integer]: string read GetName2;
    property Values[const Key: string]: string read GetValue2 write SetValue2; default;
    property Low: Integer read GetLow;
    class property Kind: string read GetKind2;
    procedure Run;
    class procedure ClassRun;
  end;

function TBase.GetName(I: Integer): string;
begin
  Result := FNames[I];
end;

procedure TBase.SetName(I: Integer; const Value: string);
begin
  WriteLn('TBase.SetName ', I, ' ', Value);
  FNames[I] := Value;
end;

function TBase.GetValue(const Key: string): string;
begin
  Result := 'base:' + Key;
end;

procedure TBase.SetValue(const Key: string; const Value: string);
begin
  WriteLn('TBase.SetValue ', Key, '=', Value);
end;

function TBase.GetPart(Index: Integer): Integer;
begin
  Result := Index * 100;
end;

class function TBase.GetKind: string;
begin
  Result := 'base kind';
end;

function TChild.GetCount: Integer;
begin
  Result := 10 + inherited Count;
end;

procedure TChild.SetCount(Value: Integer);
begin
  inherited Count := Value * 2;
end;

function TChild.GetName2(I: Integer): string;
begin
  Result := '<' + inherited Names[I] + '>';
end;

function TChild.GetValue2(const Key: string): string;
begin
  Result := 'child(' + inherited Values[Key] + ')';
end;

procedure TChild.SetValue2(const Key: string; const Value: string);
begin
  inherited Values[Key] := Value + '!';
end;

function TChild.GetLow: Integer;
begin
  Result := inherited Low + 1;
end;

class function TChild.GetKind2: string;
begin
  Result := 'child kind';
end;

class procedure TChild.ClassRun;
begin
  // A class property through `inherited` from a class method
  WriteLn(inherited Kind, ' / ', Kind);
end;

procedure TChild.Run;

  procedure Nested;
  begin
    inherited Names[2] := 'nested';
    WriteLn(inherited Names[2], ' ', Names[2]);
  end;

begin
  inherited Names[0] := 'zero';
  WriteLn(inherited Names[0], ' ', Names[0]);
  WriteLn(inherited Count, ' ', Count, ' ', inherited Count + Count);
  WriteLn(inherited Low, ' ', Low);
  WriteLn(inherited Kind, ' / ', Kind);
  Nested;
end;

var
  C: TChild;
begin
  C := TChild.Create;
  C.Count := 3;
  WriteLn(C.Count);
  WriteLn(C['k']);
  C['x'] := 'y';
  WriteLn(TBase(C)['k']);
  C.Run;
  TChild.ClassRun;
  C.Free;
end.
