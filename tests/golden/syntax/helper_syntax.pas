program HelperSyntax;

// The syntax of helpers (objects.md 10.1): `class helper` and `record helper` with a parent in
// parentheses, the type after `for` and a class body. After `record`, `helper` followed by `,`
// or `:` is a field. A helper has no forward declaration (E0400 expects `end`), and like a class
// it is declared only in a `type` section (E0404 elsewhere, skipped to its `end`).

type
  TFoo = class
  end;
  TFooHelper = class helper for TFoo
    procedure Run;
  end;
  TMore = class helper(TFooHelper) for TFoo
  end;
  TText = record helper for string
  private
    class var Count: Integer;
  public
    function Size: Integer;
    property Len: Integer read Size;
  end;
  TPair = record
    helper, other: Integer;
  end;
  TBad = class helper for TFoo;

var
  V: class helper for TFoo procedure X; end;

procedure TFooHelper.Run;
begin
end;

function TText.Size: Integer;
begin
  Result := 0;
end;

begin
end.
