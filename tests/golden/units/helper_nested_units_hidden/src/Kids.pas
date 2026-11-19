unit Kids;

// Uses `Far`, and declares a descendant of its outer type and an alias of it in a class.

interface

uses Far;

type
  TChild = class(TFarOuter)
  end;

  TOther = class
  type
    TAlias = TFarOuter;
  end;

implementation

end.
