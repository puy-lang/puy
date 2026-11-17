program DumpGenericDecl;

// The `hir` of generic declarations (generics.md 8): the type parameters and their constraints
// on the `.type` line, the members with their types as `T`, and the bodies of the methods as
// `.routine -` (no function) checked with their type parameters. A program that only declares
// generic types runs, and its function table has no generic body (`bytecode`).

type
  TStack<T> = class
  private
    FItems: TArray<T>;
    FCount: Integer;
  public
    procedure Push(const Item: T);
    function Pop: T;
    function Peek: T; virtual;
  end;
  TPair<K, V> = record
    Key: K;
    Value: V;
    procedure Clear;
  end;
  TOwnedList<T: class, NotRefCounted> = class
    procedure Clear;
  end;

procedure TStack<T>.Push(const Item: T);
begin
  if FCount = Length(FItems) then
    SetLength(FItems, FCount * 2 + 4);
  FItems[FCount] := Item;
  Inc(FCount);
end;

function TStack<T>.Pop: T;
begin
  Dec(FCount);
  Result := FItems[FCount];
  FItems[FCount] := Default(T);
end;

function TStack<T>.Peek: T;
var
  Last: T;
begin
  Last := Default(T);
  for var Item in FItems do
    Last := Item;
  Exit(Last);
end;

procedure TPair<K, V>.Clear;
begin
  Key := Default(K);
  Value := Default(V);
end;

procedure TOwnedList<T>.Clear;
begin
end;

begin
  WriteLn('declared');
end.
