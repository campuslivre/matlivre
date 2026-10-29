classdef Point
    % A 2-D point with value semantics, a dependent property and operator overloading.
    properties
        X = 0
        Y = 0
    end
    properties (Dependent)
        R
    end
    methods
        function p = Point(x, y)
            if nargin > 0
                p.X = x;
                p.Y = y;
            end
        end
        function r = get.R(p)
            r = hypot(p.X, p.Y);
        end
        function p = plus(a, b)
            p = Point(a.X + b.X, a.Y + b.Y);
        end
    end
end
