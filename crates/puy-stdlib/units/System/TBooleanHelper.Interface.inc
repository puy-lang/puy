// ===== TBooleanHelper (docs/spec/stdlib/System/Helpers.md) =====

type
  // The members of `Boolean` besides the built-in ToString.
  TBooleanHelper = record helper for Boolean
  public
    class function Parse(const S: string): Boolean; static;
    class function TryParse(const S: string; out Value: Boolean): Boolean; static;

    function ToInteger: Integer;
  end;
