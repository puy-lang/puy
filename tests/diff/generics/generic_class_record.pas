program GenericClassRecord;

// Generic classes and records that run the same under FPC and Delphi (generics.md 4): fields
// and methods of several instantiations, a dynamic array field grown with `SetLength`,
// `Default(T)`, a property, a generic type used inside a generic body, and a record method.

type
  TPair<K, V> = record
    Key: K;
    Value: V;
    procedure Clear;
  end;

  TBox<T> = class
  private
    FItem: T;
  public
    constructor Create(const AItem: T);
    function Get: T;
  end;

  TStack<T> = class
  private
    FItems: array of T;
    FCount: Integer;
    function GetTop: T;
  public
    procedure Push(const Item: T);
    function Pop: T;
    function Boxed: TBox<T>;
    property Count: Integer read FCount;
    property Top: T read GetTop;
  end;

procedure TPair<K, V>.Clear;
begin
  Key := Default(K);
  Value := Default(V);
end;

constructor TBox<T>.Create(const AItem: T);
begin
  inherited Create;
  FItem := AItem;
end;

function TBox<T>.Get: T;
begin
  Result := FItem;
end;

function TStack<T>.GetTop: T;
begin
  Result := FItems[FCount - 1];
end;

procedure TStack<T>.Push(const Item: T);
begin
  if FCount = Length(FItems) then
    SetLength(FItems, FCount * 2 + 1);
  FItems[FCount] := Item;
  Inc(FCount);
end;

function TStack<T>.Pop: T;
begin
  Dec(FCount);
  Result := FItems[FCount];
  FItems[FCount] := Default(T);
end;

function TStack<T>.Boxed: TBox<T>;
begin
  Result := TBox<T>.Create(GetTop);
end;

var
  Ints: TStack<Integer>;
  Strs: TStack<string>;
  Box: TBox<Integer>;
  P: TPair<string, Integer>;
  Q: TPair<Integer, string>;
  I: Integer;
begin
  Ints := TStack<Integer>.Create;
  for I := 1 to 6 do
    Ints.Push(I * 10);
  WriteLn(Ints.Count, ' ', Ints.Top, ' ', Length(Ints.FItems));
  Box := Ints.Boxed;
  WriteLn(Box.Get);
  Box.Free;
  while Ints.Count > 0 do
    Write(Ints.Pop, ' ');
  WriteLn;
  WriteLn(Ints.FItems[0]);
  Ints.Free;

  Strs := TStack<string>.Create;
  Strs.Push('x');
  Strs.Push('y');
  WriteLn(Strs.Pop, Strs.Top, ' ', Strs.Count);
  WriteLn('[', Strs.FItems[1], ']');
  Strs.Free;

  P.Key := 'k';
  P.Value := 7;
  Q.Key := P.Value;
  Q.Value := P.Key;
  WriteLn(Q.Key, ' ', Q.Value);
  Q.Clear;
  WriteLn(Q.Key, ' [', Q.Value, ']');
end.
