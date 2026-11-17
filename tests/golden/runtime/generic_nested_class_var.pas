program GenericNestedClassVar;

// Types nested in a generic type and the `class var`s of instantiations (generics.md 2.5): a
// nested enumerator class using the outer type's type parameter (`for … in` over two
// instantiations), a nested type that is another type for each instantiation of the outer type
// (its `class var` counts apart), a nested array type, `class var`s, a class constant and a
// `class property` that each instantiation has on its own, and a generic record nested in a class
// that is not generic, used inside it and as `THolder.TBox<string>`.

type
  TList<T> = class
  public
    type
      TEnumerator = class
      private
        FList: TList<T>;
        FIndex: Integer;
      public
        constructor Create(AList: TList<T>);
        function MoveNext: Boolean;
        function GetCurrent: T;
        property Current: T read GetCurrent;
      end;
      TItems = array of T;
      TNode = class
      public
        Value: T;
        class var Made: Integer;
      end;
  private
    FItems: TItems;
  public
    procedure Add(const X: T);
    function GetEnumerator: TEnumerator;
    function NewNode(const X: T): TNode;
  end;

  TCounter<T> = class
  public
    class var Count: Integer;
    class var Last: T;
    const Tag = 'counter';
    constructor Create(const X: T);
    class function Total: Integer; static;
    class property Recent: T read Last;
  end;

  THolder = class
  public
    type
      TBox<T> = record
        V: T;
        function Show: string;
      end;
    var
      B: TBox<Integer>;
  end;

constructor TList<T>.TEnumerator.Create(AList: TList<T>);
begin
  FList := AList;
  FIndex := -1;
end;

function TList<T>.TEnumerator.MoveNext: Boolean;
begin
  Inc(FIndex);
  Result := FIndex < Length(FList.FItems);
end;

function TList<T>.TEnumerator.GetCurrent: T;
begin
  Result := FList.FItems[FIndex];
end;

procedure TList<T>.Add(const X: T);
begin
  SetLength(FItems, Length(FItems) + 1);
  FItems[High(FItems)] := X;
end;

function TList<T>.GetEnumerator: TEnumerator;
begin
  Result := TEnumerator.Create(Self);
end;

function TList<T>.NewNode(const X: T): TNode;
begin
  Result := TNode.Create;
  Result.Value := X;
  Inc(TNode.Made);
end;

constructor TCounter<T>.Create(const X: T);
begin
  Inc(Count);
  Last := X;
end;

class function TCounter<T>.Total: Integer;
begin
  Result := Count;
end;

function THolder.TBox<T>.Show: string;
begin
  Result := 'box';
end;

var
  L: TList<string>;
  N: TList<Integer>;
  S: string;
  I: Integer;
  A, B: TCounter<Integer>;
  C: TCounter<string>;
  H: THolder;
  X: THolder.TBox<string>;
begin
  L := TList<string>.Create;
  L.Add('a');
  L.Add('b');
  for S in L do
    Write(S);
  WriteLn;
  N := TList<Integer>.Create;
  N.Add(4);
  N.Add(5);
  for I in N do
    Write(I, ' ');
  WriteLn;
  L.NewNode('x').Free;
  N.NewNode(1).Free;
  N.NewNode(2).Free;
  WriteLn(TList<string>.TNode.Made, ' ', TList<Integer>.TNode.Made);
  L.Free;
  N.Free;

  A := TCounter<Integer>.Create(1);
  B := TCounter<Integer>.Create(2);
  C := TCounter<string>.Create('s');
  WriteLn(TCounter<Integer>.Count, ' ', TCounter<string>.Count, ' ', TCounter<Integer>.Total);
  WriteLn(TCounter<Integer>.Last, ' ', TCounter<string>.Recent, ' ', TCounter<string>.Tag);
  A.Free;
  B.Free;
  C.Free;

  H := THolder.Create;
  H.B.V := 3;
  X.V := 'x';
  WriteLn(H.B.V, X.V, X.Show);
  H.Free;
end.
