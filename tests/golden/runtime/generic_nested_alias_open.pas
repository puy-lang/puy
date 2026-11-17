program GenericNestedAliasOpen;

// Inside a generic type, a nested type of another generic type's instantiation can be named
// through an alias while the type parameter is still open, even when both are declared in the
// same type section (generics.md 2.5, 4.2): in an alias of the section, a field type, a method
// heading and a local variable, and for a nested type declared by the parent (itself an
// instantiation of the same section). It is the same type as the one named from outside.

uses System.Generics.Collections;

type
  TBase<T> = class
  public
    type
      TPair = record
        Left, Right: T;
      end;
  end;

  TBox<T> = class(TBase<T>)
  public
    type
      TItem = record
        Value: T;
        Tag: string;
      end;
      TCursor = class
        Pos: Integer;
        function Next: Integer;
      end;
  end;

  TUser<T> = class
  private
    type
      TB = TBox<T>;
      TI = TB.TItem;
  public
    First: TB.TItem;
    Second: TI;
    Cursor: TB.TCursor;
    Pair: TB.TPair;
    constructor Create(const A, B: T);
    destructor Destroy; override;
    function Pick(UseFirst: Boolean): TB.TItem;
    function Sum: string;
  end;

  // The set's enumerator is the enumerator of the dictionary that holds its items
  TSet = THashSet<string>;
  TBoxI = TBox<Integer>;

function TBox<T>.TCursor.Next: Integer;
begin
  Inc(Pos);
  Result := Pos;
end;

constructor TUser<T>.Create(const A, B: T);
begin
  First.Value := A;
  First.Tag := 'first';
  Second.Value := B;
  Second.Tag := 'second';
  Cursor := TB.TCursor.Create;
  Pair.Left := A;
  Pair.Right := B;
end;

destructor TUser<T>.Destroy;
begin
  Cursor.Free;
  inherited Destroy;
end;

function TUser<T>.Pick(UseFirst: Boolean): TB.TItem;
var
  Item: TI;
begin
  if UseFirst then
    Item := First
  else
    Item := Second;
  Result := Item;
end;

function TUser<T>.Sum: string;
begin
  Result := First.Tag + '+' + Second.Tag;
end;

var
  U: TUser<Integer>;
  S: TUser<string>;
  Item: TBoxI.TItem;
  Pair: TBoxI.TPair;
  Names: TSet;
  E: TSet.TEnumerator;
begin
  U := TUser<Integer>.Create(3, 4);
  Item := U.Pick(False);
  WriteLn(Item.Tag, ' ', Item.Value);
  Item := U.First;
  WriteLn(Item.Tag, ' ', Item.Value, ' ', U.Sum);
  WriteLn(U.Cursor.Next, ' ', U.Cursor.Next);
  Pair := U.Pair;
  WriteLn(Pair.Left, ' ', Pair.Right);
  U.Free;

  S := TUser<string>.Create('x', 'y');
  WriteLn(S.Pick(True).Value, S.Pick(False).Value, S.Pair.Right, S.Pair.Left);
  S.Free;

  Names := TSet.Create(['b', 'a', 'c']);
  E := Names.GetEnumerator;
  while E.MoveNext do
    Write(E.Current, ' ');
  WriteLn;
  Names.Free;
end.
