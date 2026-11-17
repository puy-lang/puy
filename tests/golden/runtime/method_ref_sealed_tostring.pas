program MethodRefSealedToString;

// A method reference to `ToString` of a `sealed` class (objects.md 9.3): the function is known
// statically, and the built-in body (not overridden) is still a value; an overriding body is too.

type
  TStrFn = function: string of object;

  TLeaf = class sealed
  end;

  TNamed = class sealed
    function ToString: string; override;
  end;

function TNamed.ToString: string;
begin
  Result := 'named';
end;

var
  L: TLeaf;
  N: TNamed;
  F: TStrFn;
begin
  L := TLeaf.Create;
  N := TNamed.Create;
  F := L.ToString;
  WriteLn(F());
  F := N.ToString;
  WriteLn(F());
  L.Free;
  N.Free;
end.
