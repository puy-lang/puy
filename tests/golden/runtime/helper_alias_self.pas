program HelperAliasSelf;

// A helper for an alias is a helper for the type itself (objects.md 10.2): a helper for
// `TName = string` applies to a `string` and the other way round. In a helper's body,
// `Self.Name` searches all the helpers (10.4), so it reaches a member of another helper and the
// built-in `Length`.

type
  TName = string;

  TNameHelper = record helper for TName
    function Greet: string;
  end;

  TTextHelper = record helper for string
    function Twice: string;
    function Both: string;
  end;

function TNameHelper.Greet: string;
begin
  Result := 'hello ' + Self;
end;

function TTextHelper.Twice: string;
begin
  Result := Self + Self;
end;

function TTextHelper.Both: string;
begin
  Result := Self.Greet + ' / ' + Self.Twice + ' / ' + Self.Length.ToString;
end;

var
  S: string;
  N: TName;
begin
  S := 'ann';
  N := 'bob';
  WriteLn(S.Greet);
  WriteLn(N.Twice);
  WriteLn(S.Both);
end.
