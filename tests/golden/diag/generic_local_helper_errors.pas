// A helper declared in a generic body (generics.md 2.1, 2.6) is E0757: a helper is not copied
// for each instantiation, so a local type it is for would stay the open type. The body of a
// generic routine, of a method of a generic type and of a routine nested in one are the same, and
// so is a helper for a type declared outside, and one nested in a local class (reported as the
// body it is in, not as a generic type). E0757 is the only error: the bodies of the helper's
// methods and uses of the helper add no other.
program GenericLocalHelperErrors;

function Find<T>(const X: T): Integer;
type
  TLoc = record
    V: T;
    N: Integer;
  end;
  TLocHelper = record helper for TLoc
    function Twice: Integer;
  end;
function TLocHelper.Twice: Integer;
begin
  Result := N * 2;
end;
var
  L: TLoc = Default(TLoc);
begin
  L.V := X;
  Result := L.Twice;
end;

function Outer<T>(const X: T): Integer;

  function Inner: Integer;
  type
    TIntHelper = record helper for Integer
      function Twice: Integer;
    end;
  function TIntHelper.Twice: Integer;
  var
    Y: T = Default(T);
  begin
    Result := Self * 2;
  end;
  begin
    Result := 2.Twice;
  end;

begin
  Result := Inner;
end;

type
  TBox<T> = class
    procedure Run;
  end;

function Nested<T>(const X: T): Integer;
type
  TLocal = class
  type
    TNestedHelper = class helper for TObject
      function Z: Integer;
    end;
  end;
begin
  Result := 1;
end;

procedure TBox<T>.Run;
type
  TInner = class
  type
    TInnerHelper = class helper for TObject
      function Z: Integer;
    end;
  end;
  TObjHelper = class helper for TObject
    function Tag: string;
  end;
function TObjHelper.Tag: string;
begin
  Result := 'tag';
end;
begin
  WriteLn(Self.Tag);
end;

begin
  WriteLn(Find(1));
  WriteLn(Outer(1));
  WriteLn(Nested(1));
  TBox<Integer>.Create.Run;
end.
