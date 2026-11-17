program DumpGenericInstances;

// The instantiations do not box their values (design generics.md 3.4): `TList<Integer>` adds
// with the instructions that do not count (`DynSet`), as the routine `AddInt` written by hand
// over `array of Integer` does, `TList<string>` with the `…Ref` ones, and the classes, the
// record and the reference counted classes each with the instructions of their static type. No
// instantiation has `AssignObj` (the run-time check of `TObject`). The instantiations' functions
// come after every unit's (generics.md 7); those that come out the same are shared under the
// generic name with a `; shared by` line: the two classes managed by hand, the two reference
// counted classes, and `Integer` with `Int64`. `Integer` and `string` are not shared. The `hir`
// shows the instantiations the file asked for with their bodies (`.instance`, generics.md 8),
// before sharing.

type
  TList<T> = class
  private
    FItems: TArray<T>;
    FCount: Integer;
  public
    procedure Add(const X: T);
    function Get(I: Integer): T;
  end;

  TPoint = record
    X, Y: Integer;
  end;
  TFoo = class
  end;
  TBar = class
  end;
  TRef = class(TRefObject)
  end;
  TRef2 = class(TRefObject)
  end;

procedure TList<T>.Add(const X: T);
begin
  if FCount = Length(FItems) then
    SetLength(FItems, FCount * 2 + 4);
  FItems[FCount] := X;
  Inc(FCount);
end;

function TList<T>.Get(I: Integer): T;
begin
  Result := FItems[I];
end;

procedure AddInt(var Items: TArray<Integer>; var Count: Integer; const X: Integer);
begin
  if Count = Length(Items) then
    SetLength(Items, Count * 2 + 4);
  Items[Count] := X;
  Inc(Count);
end;

var
  Ints: TList<Integer>;
  Strs: TList<string>;
  Points: TList<TPoint>;
  Foos: TList<TFoo>;
  Bars: TList<TBar>;
  Refs: TList<TRef>;
  Refs2: TList<TRef2>;
  Longs: TList<Int64>;
  Items: TArray<Integer>;
  Count: Integer;
  P: TPoint;
begin
  Ints := TList<Integer>.Create;
  Ints.Add(1);
  Strs := TList<string>.Create;
  Strs.Add('a');
  Points := TList<TPoint>.Create;
  P := Default(TPoint);
  Points.Add(P);
  Foos := TList<TFoo>.Create;
  Foos.Add(nil);
  Bars := TList<TBar>.Create;
  Bars.Add(nil);
  Refs := TList<TRef>.Create;
  Refs.Add(TRef.Create);
  Refs2 := TList<TRef2>.Create;
  Refs2.Add(TRef2.Create);
  Longs := TList<Int64>.Create;
  Longs.Add(3);
  Count := 0;
  AddInt(Items, Count, 2);
  WriteLn(Ints.Get(0), Strs.Get(0), Points.Get(0).X, Longs.Get(0), Length(Items));
  Ints.Free;
  Strs.Free;
  Points.Free;
  Foos.Free;
  Bars.Free;
  Refs.Free;
  Refs2.Free;
  Longs.Free;
end.
