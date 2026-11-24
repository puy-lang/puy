program DumpTValue;

// TValue.From<T> and AsType<T> are instantiated for each type with the instructions of that
// type (rtti.md 3.3, 3.5, chapter 8): an Integer goes to FBits, a string to FRef, an instance of a
// class managed by hand to FBits with the class of the instance (ValueOfObject), TObject looks
// at the instance; AsType reads the slot back after ValueMatch. Only the types made into TValues
// and the classes have type information (1.2)

type
  TFoo = class
  end;

var
  V: TValue;
  F: TFoo;
begin
  V := TValue.From<Integer>(1);
  WriteLn(V.AsType<Integer>);
  V := TValue.From<string>('s');
  WriteLn(V.AsType<string>);
  F := TFoo.Create;
  V := TValue.From<TFoo>(F);
  WriteLn(V.AsType<TFoo> = F);
  V := TValue.From<TObject>(F);
  F.Free;
end.
