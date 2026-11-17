program HelperDescendantOwn;

// A descendant's own member wins over a helper of its ancestor with the same name (objects.md
// 10.3, 10.4 step 1): `TB.Hi` is `TB`'s, while a `TA` value (also a `TB` seen as `TA`) gets the
// helper's.

type
  TA = class
  end;

  TAHelper = class helper for TA
    function Hi: string;
  end;

  TB = class(TA)
    function Hi: string;
  end;

function TAHelper.Hi: string;
begin
  Result := 'helper of TA';
end;

function TB.Hi: string;
begin
  Result := 'own of TB';
end;

var
  A: TA;
  B: TB;
begin
  A := TA.Create;
  B := TB.Create;
  WriteLn(A.Hi);
  WriteLn(B.Hi);
  WriteLn(TA(B).Hi);
  A.Free;
  B.Free;
end.
