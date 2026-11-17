program ClassProperties;

// Properties (objects.md 2.6): fields and methods as `read` / `write`, indexed properties (one
// and two indices), `index N` passed first to the accessor, `class property` with a `class var`
// and a static class method, a virtual `write` method chosen by the instance's class, properties
// used without `Self.` inside a method, and the order of evaluation of `O.P := X` and
// `O.Items[I]` (the instance, the index, the value, then the call).

type
  TCounter = class
  private
    FCount: Integer;
    FNames: array[0..2] of string;
    procedure SetCount(Value: Integer);
    function GetItem(Index: Integer): string;
    procedure SetItem(Index: Integer; const Value: string);
    function GetScaled(Index: Integer): Integer;
    function GetCell(I, J: Integer): Integer;
    procedure SetCell(I, J: Integer; Value: Integer);
  public
    class var FTotal: Integer;
    class function GetTotal: Integer; static;
    property Count: Integer read FCount write SetCount;
    property Raw: Integer read FCount write FCount;
    property Items[Index: Integer]: string read GetItem write SetItem;
    property Twice: Integer index 2 read GetScaled;
    property Thrice: Integer index 3 read GetScaled;
    property Cells[I, J: Integer]: Integer read GetCell write SetCell;
    class property Total: Integer read FTotal write FTotal;
    class property TotalPlus: Integer read GetTotal;
  end;

  TBase = class
  private
    FValue: Integer;
  protected
    procedure SetValue(V: Integer); virtual;
    function GetItem(I: Integer): Integer; virtual;
  public
    property Value: Integer read FValue write SetValue;
    property Items[I: Integer]: Integer read GetItem;
    procedure Show;
  end;

  TChild = class(TBase)
  protected
    procedure SetValue(V: Integer); override;
    function GetItem(I: Integer): Integer; override;
  end;

procedure TCounter.SetCount(Value: Integer);
begin
  WriteLn('SetCount ', Value);
  FCount := Value;
end;

function TCounter.GetItem(Index: Integer): string;
begin
  Result := FNames[Index];
end;

procedure TCounter.SetItem(Index: Integer; const Value: string);
begin
  FNames[Index] := Value;
end;

function TCounter.GetScaled(Index: Integer): Integer;
begin
  Result := FCount * Index;
end;

function TCounter.GetCell(I, J: Integer): Integer;
begin
  Result := I * 10 + J;
end;

procedure TCounter.SetCell(I, J: Integer; Value: Integer);
begin
  WriteLn('SetCell ', I, ' ', J, ' ', Value);
end;

class function TCounter.GetTotal: Integer;
begin
  Result := FTotal + 100;
end;

procedure TBase.SetValue(V: Integer);
begin
  WriteLn('TBase.SetValue ', V);
  FValue := V;
end;

function TBase.GetItem(I: Integer): Integer;
begin
  Result := I;
end;

procedure TBase.Show;
begin
  Value := Value + 1;
  WriteLn('Show ', Value, ' ', Items[3], ' ', Self.Items[4]);
end;

procedure TChild.SetValue(V: Integer);
begin
  WriteLn('TChild.SetValue ', V);
  inherited SetValue(V * 2);
end;

function TChild.GetItem(I: Integer): Integer;
begin
  Result := I * 100;
end;

var
  C: TCounter;
  B: TBase;

function GetObj: TBase;
begin
  WriteLn('object');
  Result := B;
end;

function GetVal: Integer;
begin
  WriteLn('value');
  Result := 42;
end;

function GetIdx: Integer;
begin
  WriteLn('index');
  Result := 1;
end;

begin
  C := TCounter.Create;
  C.Count := 5;
  WriteLn(C.Count, ' ', C.Twice, ' ', C.Thrice);
  C.Raw := 7;
  WriteLn(C.Raw, ' ', C.Count);
  C.Items[1] := 'one';
  C.Items[2] := C.Items[1] + 'two';
  WriteLn(C.Items[1], ' ', C.Items[2], ' ', Length(C.Items[2]));
  WriteLn(C.Cells[2, 3]);
  C.Cells[4, 5] := C.Cells[1, 2];
  TCounter.Total := 4;
  C.Total := C.Total + 1;
  WriteLn(TCounter.Total, ' ', TCounter.TotalPlus, ' ', C.TotalPlus);
  C.Free;

  B := TChild.Create;
  GetObj.Value := GetVal;
  WriteLn(B.Value);
  B.Show;
  WriteLn(GetObj.Items[GetIdx]);
  B.Free;
end.
