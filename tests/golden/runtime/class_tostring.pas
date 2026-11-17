program ClassToString;

// `ToString` (objects.md chapter 5, 14.3): the built-in body returns the declared class name; an
// override replaces it for the class and its descendants, also called through a `TObject`
// variable; `inherited ToString` in an override is the class name of the instance; `ClassName`
// does not change with an override. Here rather than in diff/, as FPC's `TObject.ToString` does
// not return `string` in delphiunicode mode.

type
  TPlain = class
  end;

  TNamed = class
    Name: string;
    function ToString: string; override;
  end;

  TTagged = class(TNamed)
  end;

function TNamed.ToString: string;
begin
  Result := Name + ' (' + inherited ToString + ')';
end;

var
  P: TPlain;
  N: TNamed;
  O: TObject;
begin
  P := TPlain.Create;
  WriteLn(P.ToString);
  N := TTagged.Create;
  N.Name := 'tag';
  O := N;
  WriteLn(O.ToString);
  WriteLn(N.ToString, ' ', N.ClassName, ' ', O.ClassName);
  P.Free;
  N.Free;
end.
