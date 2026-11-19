program NilArgOfUnresolvedCall;

// A `nil` argument of a call whose target does not resolve (an undeclared name, a constructor
// the code cannot see, the built-in `TObject.Create` with arguments) reports only the call's
// error: there is no parameter to give `nil` a type, so no E0702 follows, and an array
// constructor gets no E0729 (semantics.md chapter 3). The same holds for the arguments of a
// built-in routine called with the wrong number of arguments (`FreeAndNil`). The other arguments
// are still checked (`Missing` is reported).

uses System.SysUtils, System.Generics.Collections;

var
  O: TObject;
begin
  TFoo.Create(nil);
  O := TFoo.Create(nil);
  Foo(nil, (nil), Missing);
  O := Foo(nil);
  TDictionary<string, Integer>.TKeyCollection.Create(nil);
  O := TDictionary<string, Integer>.TKeyCollection.Create(nil);
  TObject.Create(nil);
  Foo([1, 2]);
  TObject.Create([1]);
  FreeAndNil(nil, O);
end.
