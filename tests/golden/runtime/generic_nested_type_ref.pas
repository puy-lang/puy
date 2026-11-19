program GenericNestedTypeRef;

// A type nested in an instantiation can be written in a type position right after the type
// arguments (`TList<Integer>.TEnumerator`, `TOuter<Integer>.TInner<string>`; grammar.md 1.3,
// generics.md 2.5): in a variable, a parameter, a result, a field, a type argument, an alias
// and a constant's type. It is the same type as the one named through an alias of the
// instantiation, so values of the two forms are assignable to each other.

uses System.Generics.Collections;

type
  TOuter<T> = class
  public
    type
      TInner<U> = class
      public
        type
          TPair = record
            Left: T;
            Right: U;
          end;
      public
        Pair: TPair;
      end;
      TCell = record
        Value: T;
      end;
  end;

  TIntList = TList<Integer>;
  TIntOuter = TOuter<Integer>;
  TKeys = TDictionary<string, Integer>.TKeyCollection;
  TDeepPair = TOuter<Integer>.TInner<string>.TPair;

  THolder = class
  public
    Cursor: TList<Integer>.TEnumerator;
    Cells: TArray<TOuter<Double>.TCell>;
  end;

function CountKeys(const Keys: TDictionary<string, Integer>.TKeyCollection): Integer;
var
  S: string;
begin
  Result := 0;
  for S in Keys do
    Inc(Result);
end;

function FirstCursor(L: TList<Integer>): TList<Integer>.TEnumerator;
begin
  Result := L.GetEnumerator;
end;

procedure Drain(var E: TIntList.TEnumerator);
begin
  while E.MoveNext do
    Write(E.Current, ' ');
  WriteLn;
end;

const
  Origin: TOuter<Integer>.TCell = (Value: 7);

var
  L: TList<Integer>;
  E: TList<Integer>.TEnumerator;
  D: TDictionary<string, Integer>;
  K: TKeys;
  Inner: TOuter<Integer>.TInner<string>;
  Same: TIntOuter.TInner<string>;
  Pair: TDeepPair;
  Cursors: TList<TList<Integer>.TEnumerator>;
  H: THolder;
begin
  L := TList<Integer>.Create;
  L.Add(1);
  L.Add(2);
  L.Add(3);

  // A result of the written form passed as a `var` argument of the alias form
  E := FirstCursor(L);
  Drain(E);

  D := TDictionary<string, Integer>.Create;
  D.Add('a', 1);
  D.Add('b', 2);
  K := D.Keys;
  WriteLn(CountKeys(K), ' ', CountKeys(D.Keys));

  // A generic type nested in a generic type, through the written form and an alias
  Inner := TIntOuter.TInner<string>.Create;
  Inner.Pair.Left := 4;
  Inner.Pair.Right := 'four';
  Same := Inner;
  Pair := Same.Pair;
  WriteLn(Pair.Left, ' ', Pair.Right);

  // In a type argument, also in an expression
  Cursors := TList<TList<Integer>.TEnumerator>.Create;
  Cursors.Add(L.GetEnumerator);
  H := THolder.Create;
  H.Cursor := Cursors[0];
  while H.Cursor.MoveNext do
    Write(H.Cursor.Current * 10, ' ');
  WriteLn;

  SetLength(H.Cells, 2);
  H.Cells[1].Value := 2.5;
  WriteLn(H.Cells[1].Value:0:1, ' ', Origin.Value);

  H.Free;
  Cursors.Free;
  Inner.Free;
  D.Free;
  L.Free;
end.
