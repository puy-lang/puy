program DumpProperties;

// The `hir` dump of properties (objects.md 15.1): the `.property` lines of a class (a field, a
// method, none, an index, `index N` and a `class property` with a `class var`), and the
// `property` assignment target with the receiver and the indices; a write through a field is an
// `objfield` target and a read through a method is a method call.

type
  TBox = class
  private
    FSize: Integer;
    procedure SetSize(Value: Integer);
    function GetItem(I: Integer): string;
    procedure SetItem(I: Integer; const Value: string);
    function GetScaled(Index: Integer): Integer;
  public
    class var FCount: Integer;
    property Size: Integer read FSize write SetSize;
    property Raw: Integer read FSize write FSize;
    property Items[I: Integer]: string read GetItem write SetItem;
    property Twice: Integer index 2 read GetScaled;
    property Sink: Integer write SetSize;
    class property Count: Integer read FCount write FCount;
  end;

procedure TBox.SetSize(Value: Integer);
begin
  FSize := Value;
end;

function TBox.GetItem(I: Integer): string;
begin
  Result := 'item';
end;

procedure TBox.SetItem(I: Integer; const Value: string);
begin
end;

function TBox.GetScaled(Index: Integer): Integer;
begin
  Result := FSize * Index;
end;

var
  B: TBox;
begin
  B := TBox.Create;
  B.Size := 3;
  B.Raw := 4;
  B.Items[1] := 'x';
  TBox.Count := B.Twice;
  WriteLn(B.Items[2]);
  B.Free;
end.
