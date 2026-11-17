program KeyEqualsGeneric;

// KeyEquals<T> / KeyHash<T> in generic bodies (equality.md 4, 5; generics.md 6.1): they put no
// requirement on `T`, so the same body is instantiated with an integer, a string, a record, a
// class and an anonymous method type (`TProc`), each compared by its own key rules. The type
// argument is written, or inferred from values of type `T`.

type
  TPair = record
    Name: string;
    Count: Integer;
  end;

  TSet<T> = class
    Items: TArray<T>;
    procedure Add(const Item: T);
    function IndexOf(const Item: T): Integer;
    function HashOf(Index: Integer): Int64;
  end;

procedure TSet<T>.Add(const Item: T);
begin
  if IndexOf(Item) < 0 then
    Items := Items + [Item];
end;

function TSet<T>.IndexOf(const Item: T): Integer;
var
  I: Integer;
begin
  for I := 0 to High(Items) do
    if KeyEquals<T>(Items[I], Item) then
      Exit(I);
  Result := -1;
end;

function TSet<T>.HashOf(Index: Integer): Int64;
begin
  Result := KeyHash(Items[Index]);
end;

function SameKey<T>(const A, B: T): Boolean;
begin
  Result := KeyEquals(A, B) and (KeyHash<T>(A) = KeyHash(B));
end;

var
  Ints: TSet<Integer>;
  Strs: TSet<string>;
  Pairs: TSet<TPair>;
  Procs: TSet<TProc>;
  P: TPair;
  F: TProc;
begin
  Ints := TSet<Integer>.Create;
  Ints.Add(1);
  Ints.Add(2);
  Ints.Add(1);
  WriteLn('Integer: ', Length(Ints.Items), ' ', Ints.IndexOf(2), ' ', Ints.HashOf(0) = KeyHash(1));
  Strs := TSet<string>.Create;
  Strs.Add('a');
  Strs.Add('b' + 'c');
  Strs.Add('bc');
  WriteLn('string: ', Length(Strs.Items), ' ', Strs.IndexOf('bc'));
  Pairs := TSet<TPair>.Create;
  P.Name := 'x';
  P.Count := 1;
  Pairs.Add(P);
  P.Count := 2;
  Pairs.Add(P);
  P.Count := 1;
  Pairs.Add(P);
  WriteLn('record: ', Length(Pairs.Items), ' ', Pairs.IndexOf(P), ' ',
    Pairs.HashOf(0) = KeyHash(P));
  Procs := TSet<TProc>.Create;
  F := procedure begin WriteLn('called') end;
  Procs.Add(F);
  Procs.Add(F);
  Procs.Add(procedure begin WriteLn('called') end);
  WriteLn('TProc: ', Length(Procs.Items), ' ', Procs.IndexOf(F));
  WriteLn('SameKey: ', SameKey(1.5, 1.5), ' ', SameKey<string>('a', 'b'), ' ', SameKey(P, P),
    ' ', SameKey<TProc>(F, F), ' ', SameKey<TObject>(Ints, Strs));
  Ints.Free;
  Strs.Free;
  Pairs.Free;
  Procs.Free;
end.
