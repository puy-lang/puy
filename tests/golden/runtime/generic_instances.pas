program GenericInstances;

// Generic records and classes instantiated with several type arguments (generics.md 4):
// fields and methods with the type parameters replaced, a `TArray<T>` field grown by `SetLength`,
// `Default(T)` of each type argument, generic types used inside a generic body (`TBox<T>`,
// `TBox<TArray<T>>`), properties, class methods, static methods of a record, nested routines and
// anonymous methods in a generic body, and a generic class implementing an interface. A generic
// type with a constraint names itself with its own type parameters (`TNode<T>` in `TNode<T: class>`
// is the definition itself and satisfies its constraints).

uses System.SysUtils;

type
  IShow = interface
    function Show: string;
  end;

  TColor = (Red, Green, Blue);
  TStrFunc = reference to function: string;

  TPair<K, V> = record
    Key: K;
    Value: V;
    constructor Create(const AKey: K; const AValue: V);
    class function Empty: TPair<K, V>; static;
  end;

  TBox<T> = class
  private
    FItem: T;
  public
    constructor Create(const AItem: T);
    function Get: T;
  end;

  TStack<T> = class(TInterfacedObject, IShow)
  private
    FItems: TArray<T>;
    FCount: Integer;
    function GetTop: T;
  public
    procedure Push(const Item: T);
    function Pop: T;
    function Peek: T;
    function Boxed: TBox<T>;
    function Snapshot: TBox<TArray<T>>;
    function Show: string;
    class function Named: string;
    property Count: Integer read FCount;
    property Top: T read GetTop;
  end;

  TNode<T: class> = class
    Value: T;
    Next: TNode<T>;
    function Count: Integer;
  end;

function TNode<T>.Count: Integer;
var
  N: TNode<T>;
begin
  Result := 0;
  N := Self;
  while N <> nil do
  begin
    Inc(Result);
    N := N.Next;
  end;
end;

constructor TPair<K, V>.Create(const AKey: K; const AValue: V);
begin
  Key := AKey;
  Value := AValue;
end;

class function TPair<K, V>.Empty: TPair<K, V>;
begin
  Result := TPair<K, V>.Create(Default(K), Default(V));
end;

constructor TBox<T>.Create(const AItem: T);
begin
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

function TStack<T>.Peek: T;
  function Last: T;
  begin
    Result := FItems[FCount - 1];
  end;
begin
  Result := Last;
end;

function TStack<T>.Boxed: TBox<T>;
begin
  Result := TBox<T>.Create(Peek);
end;

function TStack<T>.Snapshot: TBox<TArray<T>>;
begin
  Result := TBox<TArray<T>>.Create(Copy(FItems, 0, FCount));
end;

function TStack<T>.Show: string;
var
  Describe: TStrFunc;
begin
  Describe := function: string
    begin
      Result := 'stack of ' + IntToStr(FCount);
    end;
  Result := Describe();
end;

class function TStack<T>.Named: string;
begin
  Result := ClassName;
end;

procedure Ints;
var
  S: TStack<Integer>;
  B: TBox<Integer>;
  Snap: TBox<TArray<Integer>>;
  I: Integer;
begin
  S := TStack<Integer>.Create;
  for I := 1 to 5 do
    S.Push(I * I);
  WriteLn(S.Count, ' ', S.Top, ' ', S.Peek, ' ', Length(S.FItems));
  B := S.Boxed;
  WriteLn(B.Get);
  B.Free;
  Snap := S.Snapshot;
  for I in Snap.Get do
    Write(I, ' ');
  WriteLn;
  Snap.Free;
  while S.Count > 0 do
    Write(S.Pop, ' ');
  WriteLn;
  WriteLn(S.FItems[0], ' ', TStack<Integer>.Named);
end;

procedure Strings;
var
  S: TStack<string>;
  I: IShow;
begin
  S := TStack<string>.Create;
  I := S;
  S.Push('a');
  S.Push('b');
  S.Push('c');
  WriteLn(S.Pop, S.Pop, ' ', S.Count, ' ', I.Show);
  WriteLn('[', S.FItems[1], '] ', S.Named);
end;

procedure Records;
var
  P: TPair<string, Integer>;
  Q: TPair<TColor, Double>;
  E: TPair<string, TColor>;
  Stack: TStack<TPair<string, Integer>>;
begin
  P := TPair<string, Integer>.Create('one', 1);
  WriteLn(P.Key, '=', P.Value);
  Q := Default(TPair<TColor, Double>);
  WriteLn(Ord(Q.Key), ' ', Q.Value:0:1);
  Q.Key := TColor.Blue;
  Q.Value := 2.5;
  WriteLn(Ord(Q.Key), ' ', Q.Value:0:1);
  E := TPair<string, TColor>.Empty;
  WriteLn('[', E.Key, '] ', Ord(E.Value));
  Stack := TStack<TPair<string, Integer>>.Create;
  Stack.Push(P);
  Stack.Push(TPair<string, Integer>.Create('two', 2));
  P := Stack.Pop;
  WriteLn(P.Key, ' ', Stack.Top.Key, ' ', Stack.Count);
end;

procedure Nodes;
var
  A, B: TNode<TObject>;
begin
  A := TNode<TObject>.Create;
  B := TNode<TObject>.Create;
  A.Next := B;
  WriteLn(A.Count, ' ', B.Count);
  B.Free;
  A.Free;
end;

begin
  Ints;
  Strings;
  Records;
  Nodes;
end.
