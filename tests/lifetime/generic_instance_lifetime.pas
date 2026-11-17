program GenericInstanceLifetime;

// The instantiations of generic types count their values by the type arguments' static types
// (generics.md 4.3, design generics.md 3.4): a `TList<string>`'s strings, a list of a reference
// counted class's instances and a list of records holding them are all let go of when the list
// is, with no leak, and an element overwritten or popped is let go of then.

type
  TItem = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TEntry = record
    Item: TItem;
    Tag: string;
  end;

  TList<T> = class(TRefObject)
  private
    FItems: TArray<T>;
    FCount: Integer;
  public
    destructor Destroy; override;
    procedure Add(const X: T);
    function Pop: T;
    procedure Put(I: Integer; const X: T);
    property Count: Integer read FCount;
  end;

  TCell<T> = record
    Value: T;
    Next: TArray<T>;
  end;

constructor TItem.Create(const AName: string);
begin
  Name := AName;
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited;
end;

destructor TList<T>.Destroy;
begin
  WriteLn('list destroyed with ', FCount);
  inherited;
end;

procedure TList<T>.Add(const X: T);
begin
  if FCount = Length(FItems) then
    SetLength(FItems, FCount * 2 + 1);
  FItems[FCount] := X;
  Inc(FCount);
end;

function TList<T>.Pop: T;
begin
  Dec(FCount);
  Result := FItems[FCount];
  FItems[FCount] := Default(T);
end;

procedure TList<T>.Put(I: Integer; const X: T);
begin
  FItems[I] := X;
end;

function Entry(const Name, Tag: string): TEntry;
begin
  Result := Default(TEntry);
  Result.Item := TItem.Create(Name);
  Result.Tag := Tag;
end;

procedure Strings;
var
  L: TList<string>;
  S: string;
begin
  L := TList<string>.Create;
  L.Add('a' + 'b');
  L.Add(Copy('xyzw', 1, 3));
  S := L.Pop;
  WriteLn(S, ' ', L.Count);
end;

procedure AddNamed(L: TList<TItem>; const Name: string);
begin
  L.Add(TItem.Create(Name));
end;

procedure PutNamed(L: TList<TItem>; I: Integer; const Name: string);
begin
  L.Put(I, TItem.Create(Name));
  WriteLn('put ', Name);
end;

procedure PopOne(L: TList<TItem>);
var
  Item: TItem;
begin
  Item := L.Pop;
  WriteLn('popped ', Item.Name);
end;

procedure Objects;
var
  L: TList<TItem>;
begin
  L := TList<TItem>.Create;
  AddNamed(L, 'one');
  AddNamed(L, 'two');
  AddNamed(L, 'three');
  PutNamed(L, 0, 'four');
  PopOne(L);
  WriteLn('clearing');
  L := nil;
  WriteLn('cleared');
end;

procedure Records;
var
  L: TList<TEntry>;
  C: TCell<TItem>;
begin
  L := TList<TEntry>.Create;
  L.Add(Entry('r1', 'first'));
  L.Add(Entry('r2', 'second'));
  WriteLn(L.Pop.Tag);
  C := Default(TCell<TItem>);
  C.Value := TItem.Create('cell');
  SetLength(C.Next, 2);
  C.Next[1] := TItem.Create('next');
  WriteLn('end of records');
end;

begin
  Strings;
  Objects;
  Records;
  WriteLn('end');
end.
