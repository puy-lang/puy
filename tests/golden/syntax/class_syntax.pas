program ClassSyntax;

// The CST of a class declaration (objects.md chapter 1): parents, visibility sections (a field
// named `private` is a field), fields and `var` sections, a method heading with directives, a
// property with specifiers, a forward declaration and a class reference type. Only the CST is
// checked here; semantic analysis reports the members later items implement.

type
  TFwd = class;
  TItem = class(TObject)
    private: Integer;
  strict private
    A, B: string;
  public
    var C: Integer;
    procedure Run(X: Integer); virtual; abstract;
    property Size: Integer index 1 read A write B; default;
  end;
  TFwd = class(TItem) end;
  TRef = class of TItem;

begin
end.
